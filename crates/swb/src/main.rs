//! Broker and bounded client commands. Session identity is explicit, never
//! derived by walking process ancestry (R-N11, SWB-R10).
use serde_json::{Value, json};
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::process::ExitCode;
use std::sync::mpsc;
use std::time::{Duration, Instant};

const LIMIT: Duration = Duration::from_millis(1500);
const HOOK_LIMIT: Duration = Duration::from_millis(1800);
const MAX_RESPONSE_BYTES: usize = 512 * 1024;
// A valid envelope can contain a 16 KiB body and 40 KiB of artifact paths;
// JSON escaping can enlarge the latter up to sixfold. Use one-message pages.
const CLI_INBOX_PAGE: u32 = 1;
const SEND_USAGE: &str = "usage: swb send <to-agent-id> <ticket> [--ruling ID] [--operator-directed] [--in-reply-to ULID] [--thread ULID] [--msg-id ULID] < body";

fn bounded<T, F>(limit: Duration, work: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, String> + Send + 'static,
{
    let (sender, receiver) = mpsc::sync_channel(1);
    std::thread::Builder::new()
        .name("swb-bounded-client".into())
        .spawn(move || {
            let _ = sender.send(work());
        })
        .map_err(|e| e.to_string())?;
    receiver
        .recv_timeout(limit)
        .map_err(|_| "broker deadline exceeded".to_string())?
}

fn env(name: &str) -> Result<String, String> {
    need(&process_env, name)
}

/// Where a command reads `SWB_*` settings. Production passes [`process_env`];
/// tests pass a closure so no test mutates process-wide environment.
type Lookup<'a> = &'a dyn Fn(&str) -> Option<String>;

fn process_env(name: &str) -> Option<String> {
    std::env::var(name).ok()
}

fn need(lookup: Lookup, name: &str) -> Result<String, String> {
    lookup(name)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| format!("{name} is required"))
}

fn parse_endpoint(name: &str, url: &str) -> Result<(String, u16), String> {
    let rest = url
        .strip_prefix("http://")
        .ok_or_else(|| format!("{name} must be http://"))?;
    if rest.contains(['/', '@', '?']) {
        return Err(format!("{name} must contain host and port only"));
    }
    let (host, port) = rest
        .rsplit_once(':')
        .ok_or_else(|| format!("{name} needs a port"))?;
    if host.is_empty() || host.contains(':') {
        return Err(format!("invalid {name} host"));
    }
    Ok((
        host.into(),
        port.parse().map_err(|_| format!("invalid {name} port"))?,
    ))
}

fn call(method: &str, path: &str, data: Option<&Value>) -> Result<Value, String> {
    call_with(&process_env, method, path, data)
}

fn call_with(
    lookup: Lookup,
    method: &str,
    path: &str,
    data: Option<&Value>,
) -> Result<Value, String> {
    let (host, port) = parse_endpoint("SWB_BROKER_URL", &need(lookup, "SWB_BROKER_URL")?)?;
    let method = method.to_owned();
    let path = path.to_owned();
    let data = data.cloned();
    // DNS and all socket operations run in a process-local worker. The
    // caller's deadline does not depend on resolver cancellation or EOF.
    bounded(LIMIT, move || {
        blocking_call(&host, port, &method, &path, data.as_ref())
    })
}

/// One bounded HTTP/1.1 exchange; returns the status and the raw body.
fn blocking_raw(
    host: &str,
    port: u16,
    method: &str,
    path: &str,
    data: Option<&Value>,
) -> Result<(u16, Vec<u8>), String> {
    let deadline = Instant::now() + LIMIT;
    let address = (host, port)
        .to_socket_addrs()
        .map_err(|e| e.to_string())?
        .next()
        .ok_or("broker has no address")?;
    let mut socket =
        TcpStream::connect_timeout(&address, deadline.saturating_duration_since(Instant::now()))
            .map_err(|e| e.to_string())?;
    let body = data.map_or_else(String::new, Value::to_string);
    socket
        .set_write_timeout(Some(deadline.saturating_duration_since(Instant::now())))
        .map_err(|e| e.to_string())?;
    write!(socket, "{method} {path} HTTP/1.1\r\nHost: {host}:{port}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Err("broker deadline exceeded".into());
        }
        socket
            .set_read_timeout(Some(left))
            .map_err(|e| e.to_string())?;
        let mut chunk = [0u8; 8192];
        match socket.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                bytes.extend_from_slice(&chunk[..n]);
                if bytes.len() > MAX_RESPONSE_BYTES {
                    return Err("broker response too large".into());
                }
            }
            Err(e) => return Err(e.to_string()),
        }
    }
    let split = bytes
        .windows(4)
        .position(|b| b == b"\r\n\r\n")
        .ok_or("invalid HTTP response")?;
    let headers = std::str::from_utf8(&bytes[..split]).map_err(|e| e.to_string())?;
    let status: u16 = headers
        .lines()
        .next()
        .and_then(|s| s.split_whitespace().nth(1))
        .ok_or("invalid status")?
        .parse()
        .map_err(|_| "invalid status")?;
    if headers
        .to_ascii_lowercase()
        .contains("transfer-encoding: chunked")
    {
        return Err("chunked response unsupported".into());
    }
    bytes.drain(..split + 4);
    Ok((status, bytes))
}

fn blocking_call(
    host: &str,
    port: u16,
    method: &str,
    path: &str,
    data: Option<&Value>,
) -> Result<Value, String> {
    let (status, body) = blocking_raw(host, port, method, path, data)?;
    let result: Value = serde_json::from_slice(&body).map_err(|e| e.to_string())?;
    if !(200..300).contains(&status) {
        // The broker's REST errors are short fixed strings; keep at most one
        // printable line of it so a hostile peer cannot flood the terminal.
        return Err(match result.get("error").and_then(Value::as_str) {
            Some(error) => {
                let error: String = error
                    .chars()
                    .filter(|c| !c.is_control())
                    .take(200)
                    .collect();
                format!("broker HTTP {status}: {error}")
            }
            None => format!("broker HTTP {status}"),
        });
    }
    Ok(result)
}

fn register(harness: &str, session_id: &str) -> Result<Value, String> {
    if !matches!(
        harness,
        "claude" | "kimi" | "codex" | "junie" | "opencode" | "pi"
    ) {
        return Err("invalid harness".into());
    }
    let pid: u32 = env("SWB_SESSION_PID")?
        .parse()
        .map_err(|_| "invalid SWB_SESSION_PID")?;
    if pid == 0 || session_id.is_empty() {
        return Err("missing session identity".into());
    }
    call(
        "POST",
        "/v1/register",
        Some(&json!({
            "harness":harness, "host":env("SWB_HOST")?, "pid":pid,
            "session_id":session_id, "proc_start":env("SWB_PROC_START")?
        })),
    )
}

fn inbox(me: &str) -> Result<Value, String> {
    if me.is_empty()
        || !me
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-:".contains(&b))
    {
        return Err("invalid SWB_AGENT_ID".into());
    }
    call(
        "GET",
        &format!("/v1/inbox?me={me}&limit={CLI_INBOX_PAGE}&wait_seconds=0"),
        None,
    )
}

// ---- R-C275 client verbs: send, ack, peers, doctor -------------------------
//
// Each verb reuses the bounded REST client above. Validation mirrors the
// broker's own checks (swb-store) so a malformed request fails locally with a
// readable message; the broker stays the authority and re-checks everything.

const HARNESSES: &[&str] = &["claude", "kimi", "codex", "junie", "opencode", "pi"];
const MAX_BODY_BYTES: usize = 16384;

fn valid_part(part: &str) -> bool {
    !part.is_empty()
        && part
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "._-".contains(c))
}

fn valid_agent_id(id: &str) -> bool {
    let parts: Vec<_> = id.split(':').collect();
    id.len() <= 512
        && parts.len() == 4
        && HARNESSES.contains(&parts[0])
        && parts[2].parse::<u32>().is_ok()
        && parts[1..].iter().all(|p| valid_part(p))
}

fn valid_ticket(ticket: &str) -> bool {
    ticket == "none"
        || ticket.len() > 4
            && ticket.len() <= 64
            && ticket.starts_with("TIN-")
            && ticket[4..].bytes().all(|b| b.is_ascii_digit())
}

fn valid_ulid(id: &str) -> bool {
    id.len() == 26
        && id.as_bytes()[0] <= b'7'
        && id
            .bytes()
            .all(|b| b"0123456789ABCDEFGHJKMNPQRSTVWXYZ".contains(&b))
}

fn me_from(lookup: Lookup) -> Result<String, String> {
    let me = need(lookup, "SWB_AGENT_ID")?;
    if !valid_agent_id(&me) {
        return Err("invalid SWB_AGENT_ID".into());
    }
    Ok(me)
}

/// Options for `swb send`, parsed from argv. The body is read from stdin,
/// never from argv, so it is not exposed in the process table or a shell.
#[derive(Debug, Default, PartialEq)]
struct SendArgs {
    to: String,
    ticket: String,
    ruling: Option<String>,
    operator_directed: bool,
    in_reply_to: Option<String>,
    thread_id: Option<String>,
    msg_id: Option<String>,
}

fn parse_send_args(args: &[String]) -> Result<SendArgs, String> {
    let mut out = SendArgs::default();
    let mut positional = Vec::new();
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        let mut value = |flag: &str| {
            it.next()
                .cloned()
                .ok_or_else(|| format!("{flag} needs a value"))
        };
        match arg.as_str() {
            "--ruling" => out.ruling = Some(value("--ruling")?),
            "--in-reply-to" => out.in_reply_to = Some(value("--in-reply-to")?),
            "--thread" => out.thread_id = Some(value("--thread")?),
            "--msg-id" => out.msg_id = Some(value("--msg-id")?),
            "--operator-directed" => out.operator_directed = true,
            flag if flag.starts_with("--") => return Err(format!("unknown option {flag}")),
            _ => positional.push(arg.clone()),
        }
    }
    let [to, ticket] = <[String; 2]>::try_from(positional)
        .map_err(|_| "expected <to-agent-id> <ticket>".to_string())?;
    out.to = to;
    out.ticket = ticket;
    Ok(out)
}

fn send_payload(me: &str, args: &SendArgs, body: &str) -> Result<Value, String> {
    if !valid_agent_id(&args.to) {
        return Err("invalid recipient agent id".into());
    }
    if !valid_ticket(&args.ticket) {
        return Err("invalid ticket (TIN-<digits> or none)".into());
    }
    if body.is_empty() {
        return Err("empty body (pipe it on stdin)".into());
    }
    if body.len() > MAX_BODY_BYTES {
        return Err("body exceeds 16 KiB".into());
    }
    if args.operator_directed && args.ruling.as_deref().is_none_or(str::is_empty) {
        return Err("--operator-directed requires --ruling".into());
    }
    let mut payload = json!({"from":me,"to":args.to,"ticket":args.ticket,"body":body});
    for (key, value) in [
        ("ruling", &args.ruling),
        ("in_reply_to", &args.in_reply_to),
        ("thread_id", &args.thread_id),
        ("msg_id", &args.msg_id),
    ] {
        if let Some(value) = value {
            if key != "ruling" && !valid_ulid(value) {
                return Err(format!("invalid {key} (ULID)"));
            }
            payload[key] = json!(value);
        }
    }
    if args.operator_directed {
        payload["operator_directed"] = json!(true);
    }
    Ok(payload)
}

fn read_body(input: impl Read) -> Result<String, String> {
    let mut bytes = Vec::new();
    input
        .take(MAX_BODY_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("stdin: {e}"))?;
    if bytes.len() > MAX_BODY_BYTES {
        return Err("body exceeds 16 KiB".into());
    }
    String::from_utf8(bytes).map_err(|_| "body is not UTF-8".into())
}

fn send_cmd(lookup: Lookup, args: &SendArgs, body: &str) -> Result<Value, String> {
    let payload = send_payload(&me_from(lookup)?, args, body)?;
    call_with(lookup, "POST", "/v1/send", Some(&payload))
}

fn ack_cmd(lookup: Lookup, msg_id: &str) -> Result<Value, String> {
    let me = me_from(lookup)?;
    if !valid_ulid(msg_id) {
        return Err("invalid msg_id (ULID)".into());
    }
    call_with(
        lookup,
        "POST",
        "/v1/ack",
        Some(&json!({"me":me,"msg_id":msg_id})),
    )
}

/// `swb peers` is a read-only listing: it never passes `me`, so it does not
/// refresh a lease or fail for an unregistered caller.
fn peers_cmd(lookup: Lookup) -> Result<Value, String> {
    call_with(lookup, "GET", "/v1/peers", None)
}

#[derive(Debug, PartialEq)]
enum Check {
    Ok(String),
    Skip(String),
    Fail { detail: String, fix: String },
}

fn fail(detail: impl Into<String>, fix: impl Into<String>) -> Check {
    Check::Fail {
        detail: detail.into(),
        fix: fix.into(),
    }
}

/// The identity `register` would mint, from the hook variables alone.
fn predicted_identity(lookup: Lookup) -> Result<(Value, String), Check> {
    let missing = [
        "SWB_HARNESS",
        "SWB_HOST",
        "SWB_SESSION_PID",
        "SWB_PROC_START",
        "SWB_SESSION_ID",
    ]
    .into_iter()
    .filter(|name| need(lookup, name).is_err())
    .collect::<Vec<_>>();
    if !missing.is_empty() {
        return Err(fail(
            format!("unset: {}", missing.join(", ")),
            "the lab harness launcher exports these (R-C273); enable tinyland.switchboard \
             or export them from the launcher, never by walking process ancestry",
        ));
    }
    let get = |name: &str| need(lookup, name).unwrap_or_default();
    let (harness, host, pid, proc_start, session) = (
        get("SWB_HARNESS"),
        get("SWB_HOST"),
        get("SWB_SESSION_PID"),
        get("SWB_PROC_START"),
        get("SWB_SESSION_ID"),
    );
    if !HARNESSES.contains(&harness.as_str()) {
        return Err(fail(
            format!("SWB_HARNESS={harness:?} is not a known harness"),
            format!("set SWB_HARNESS to one of {}", HARNESSES.join(", ")),
        ));
    }
    let pid_num = pid.parse::<u32>().ok().filter(|p| *p > 0);
    let Some(pid_num) = pid_num else {
        return Err(fail(
            format!("SWB_SESSION_PID={pid:?} is not a positive integer"),
            "export the launcher's own $$ before it execs the harness",
        ));
    };
    for (name, value) in [("SWB_HOST", &host), ("SWB_SESSION_ID", &session)] {
        if !valid_part(value) {
            return Err(fail(
                format!("{name} has characters outside [A-Za-z0-9._-]"),
                format!("use the short host or session name for {name}"),
            ));
        }
    }
    let agent_id = format!("{harness}:{host}:{pid}:{session}");
    if !valid_agent_id(&agent_id) {
        return Err(fail(
            "derived agent id is invalid or over 512 bytes",
            "shorten SWB_HOST or SWB_SESSION_ID",
        ));
    }
    let payload = json!({
        "harness":harness, "host":host, "pid":pid_num,
        "session_id":session, "proc_start":proc_start
    });
    Ok((payload, agent_id))
}

fn http_get_text(name: &str, url: &str, path: &str) -> Result<(u16, String), String> {
    let (host, port) = parse_endpoint(name, url)?;
    let path = path.to_owned();
    bounded(LIMIT, move || {
        let (status, body) = blocking_raw(&host, port, "GET", &path, None)?;
        Ok((status, String::from_utf8_lossy(&body).into_owned()))
    })
}

/// Runs every check in order and returns them labelled. `live_register`
/// performs the real `POST /v1/register` (an upsert of this session's own
/// row); without it the register step is a dry run that validates the exact
/// payload and prints the agent id the broker would return.
fn doctor_checks(lookup: Lookup, live_register: bool) -> Vec<(&'static str, Check)> {
    let mut checks = Vec::new();
    let broker = match need(lookup, "SWB_BROKER_URL") {
        Err(_) => fail(
            "SWB_BROKER_URL is unset",
            "export SWB_BROKER_URL=http://<broker-host>:<port> (host and port only)",
        ),
        Ok(url) => match parse_endpoint("SWB_BROKER_URL", &url) {
            Ok(_) => Check::Ok(url),
            Err(e) => fail(
                e,
                "use the form http://host:port with no path, user or query",
            ),
        },
    };
    let broker_ok = matches!(broker, Check::Ok(_));
    checks.push(("env SWB_BROKER_URL", broker));
    let identity = predicted_identity(lookup);
    checks.push((
        "env identity",
        match &identity {
            Ok((_, id)) => Check::Ok(format!("would register as {id}")),
            Err(Check::Fail { detail, fix }) => fail(detail.clone(), fix.clone()),
            Err(other) => fail(format!("{other:?}"), ""),
        },
    ));
    checks.push((
        "env SWB_AGENT_ID",
        match (lookup("SWB_AGENT_ID").filter(|v| !v.is_empty()), &identity) {
            (None, _) => Check::Skip("unset; send, ack and inbox need it".into()),
            (Some(me), _) if !valid_agent_id(&me) => fail(
                format!("{me:?} is not <harness>:<host>:<pid>:<session>"),
                "set SWB_AGENT_ID to the agent_id printed by `swb whoami`",
            ),
            (Some(me), Ok((_, id))) if &me != id => fail(
                format!("{me} differs from the registered identity {id}"),
                "re-export SWB_AGENT_ID from `swb whoami` in this session",
            ),
            (Some(me), _) => Check::Ok(me),
        },
    ));
    checks.push((
        "broker REST",
        if !broker_ok {
            Check::Skip("no valid SWB_BROKER_URL".into())
        } else {
            match peers_cmd(lookup) {
                Ok(v) if v.get("peers").is_some_and(Value::is_array) => Check::Ok(format!(
                    "GET /v1/peers listed {} session(s)",
                    v["peers"].as_array().map_or(0, Vec::len)
                )),
                Ok(_) => fail(
                    "GET /v1/peers answered without a peers list",
                    "SWB_BROKER_URL may point at something other than `swb serve`",
                ),
                Err(e) => fail(
                    format!("GET /v1/peers: {e}"),
                    "check the broker is running and reachable over the tailnet from this host",
                ),
            }
        },
    ));
    checks.push((
        "broker /metrics",
        match lookup("SWB_METRICS_URL").filter(|v| !v.is_empty()) {
            None => Check::Skip("SWB_METRICS_URL unset (metrics listen on a separate port)".into()),
            Some(url) => match http_get_text("SWB_METRICS_URL", &url, "/metrics") {
                Ok((200, text)) if text.contains("swb_sessions") => {
                    Check::Ok("GET /metrics exposes swb_* series".into())
                }
                Ok((200, _)) => fail(
                    "GET /metrics has no swb_* series",
                    "SWB_METRICS_URL should name the broker's SWB_METRICS_LISTEN address",
                ),
                Ok((status, _)) => fail(
                    format!("GET /metrics answered HTTP {status}"),
                    "SWB_METRICS_URL should name the broker's SWB_METRICS_LISTEN address",
                ),
                Err(e) => fail(
                    format!("GET /metrics: {e}"),
                    "check the metrics listener is reachable from this host",
                ),
            },
        },
    ));
    checks.push((
        "register",
        match (&identity, live_register, broker_ok) {
            (Err(_), _, _) => Check::Skip("identity incomplete".into()),
            (Ok((_, id)), false, _) => Check::Ok(format!(
                "dry run: payload valid for {id}; rerun with --register for a live round trip"
            )),
            (Ok(_), true, false) => Check::Skip("no valid SWB_BROKER_URL".into()),
            (Ok((payload, id)), true, true) => {
                match call_with(lookup, "POST", "/v1/register", Some(payload)) {
                    Ok(v) if v.get("agent_id").and_then(Value::as_str) == Some(id.as_str()) => {
                        Check::Ok(format!("POST /v1/register returned {id}"))
                    }
                    Ok(v) => fail(
                        format!("broker returned agent_id {}", v["agent_id"]),
                        "client and broker disagree on identity; upgrade both to one release",
                    ),
                    Err(e) => fail(
                        format!("POST /v1/register: {e}"),
                        "see the broker REST check",
                    ),
                }
            }
        },
    ));
    checks
}

fn render_doctor(checks: &[(&str, Check)], out: &mut impl Write) -> bool {
    let mut healthy = true;
    for (name, check) in checks {
        let _ = match check {
            Check::Ok(detail) => writeln!(out, "ok    {name}: {detail}"),
            Check::Skip(detail) => writeln!(out, "skip  {name}: {detail}"),
            Check::Fail { detail, fix } => {
                healthy = false;
                writeln!(out, "FAIL  {name}: {detail}\n      fix: {fix}")
            }
        };
    }
    healthy
}

fn print_result(verb: &str, result: Result<Value, String>) -> ExitCode {
    match result {
        Ok(v) => {
            println!("{v}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("swb {verb}: {e}");
            ExitCode::from(3)
        }
    }
}

fn hook(harness_arg: &str, event: &str) {
    if !matches!(
        event,
        "SessionStart" | "UserPromptSubmit" | "Stop" | "SessionEnd"
    ) {
        return;
    }
    let mut input = Vec::new();
    if std::io::stdin()
        .take(65537)
        .read_to_end(&mut input)
        .is_err()
        || input.len() > 65536
    {
        return;
    }
    let Ok(input) = serde_json::from_slice::<Value>(&input) else {
        return;
    };
    let Some(session_id) = input.get("session_id").and_then(Value::as_str) else {
        return;
    };
    let harness = std::env::var("SWB_HARNESS").unwrap_or_else(|_| harness_arg.into());
    if event == "SessionEnd" {
        let Ok(pid) = env("SWB_SESSION_PID") else {
            return;
        };
        let Ok(host) = env("SWB_HOST") else { return };
        let Ok(proc_start) = env("SWB_PROC_START") else {
            return;
        };
        let me = format!("{harness}:{host}:{pid}:{session_id}");
        let _ = call(
            "POST",
            "/v1/end",
            Some(&json!({"me":me,"proc_start":proc_start})),
        );
        return;
    }
    let Ok(identity) = register(&harness, session_id) else {
        return;
    };
    if event != "UserPromptSubmit" {
        return;
    }
    let Some(me) = identity.get("agent_id").and_then(Value::as_str) else {
        return;
    };
    let Ok(batch) = inbox(me) else { return };
    let Some(messages) = batch.get("messages").and_then(Value::as_array) else {
        return;
    };
    if messages.is_empty() {
        return;
    }
    let first = &messages[0];
    let sender = first
        .get("from")
        .and_then(Value::as_str)
        .unwrap_or("a peer");
    let ticket = first
        .get("ticket")
        .and_then(Value::as_str)
        .unwrap_or("none");
    let context = format!(
        "At least one unread peer message from {sender} ({ticket}) — use agents inbox to read and acknowledge it"
    );
    println!(
        "{}",
        json!({"hookSpecificOutput":{"hookEventName":"UserPromptSubmit","additionalContext":context}})
    );
}

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        Some("version") => {
            println!("swb 0.1.0 envelope v{}", swb_proto::ENVELOPE_VERSION);
            ExitCode::SUCCESS
        }
        Some("send") => {
            let rest: Vec<String> = args.collect();
            match parse_send_args(&rest) {
                Err(e) => {
                    eprintln!("swb send: {e}\n{SEND_USAGE}");
                    ExitCode::from(2)
                }
                Ok(parsed) => print_result(
                    "send",
                    read_body(std::io::stdin().lock())
                        .and_then(|body| send_cmd(&process_env, &parsed, &body)),
                ),
            }
        }
        Some("ack") => match (args.next(), args.next()) {
            (Some(msg_id), None) => print_result("ack", ack_cmd(&process_env, &msg_id)),
            _ => {
                eprintln!("usage: swb ack <msg_id>");
                ExitCode::from(2)
            }
        },
        Some("peers") => match args.next() {
            None => print_result("peers", peers_cmd(&process_env)),
            Some(_) => {
                eprintln!("usage: swb peers");
                ExitCode::from(2)
            }
        },
        Some("doctor") => {
            let live = match (args.next().as_deref(), args.next()) {
                (None, None) => false,
                (Some("--register"), None) => true,
                _ => {
                    eprintln!("usage: swb doctor [--register]");
                    return ExitCode::from(2);
                }
            };
            let checks = doctor_checks(&process_env, live);
            if render_doctor(&checks, &mut std::io::stdout().lock()) {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(3)
            }
        }
        Some("hook") => {
            if let (Some(h), Some(e), None) = (args.next(), args.next(), args.next()) {
                // Includes stdin parsing and both possible broker calls.
                // A stalled resolver/read cannot make this hook block Claude.
                let _ = bounded(HOOK_LIMIT, move || {
                    hook(&h, &e);
                    Ok(())
                });
            }
            ExitCode::SUCCESS
        }
        Some("whoami") => {
            match env("SWB_HARNESS").and_then(|h| register(&h, &env("SWB_SESSION_ID")?)) {
                Ok(v) => {
                    println!("{v}");
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("swb whoami: {e}");
                    ExitCode::from(3)
                }
            }
        }
        Some("inbox") => match env("SWB_AGENT_ID").and_then(|me| inbox(&me)) {
            Ok(v) => {
                println!("{v}");
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("swb inbox: {e}");
                ExitCode::from(3)
            }
        },
        Some("serve") => {
            let path =
                std::env::var("SWB_DB_PATH").unwrap_or_else(|_| "/var/lib/swb/swb.sqlite3".into());
            let listen = std::env::var("SWB_LISTEN").unwrap_or_else(|_| "0.0.0.0:8080".into());
            let metrics =
                std::env::var("SWB_METRICS_LISTEN").unwrap_or_else(|_| "0.0.0.0:9090".into());
            let store = swb_store::Store::open(&path).unwrap_or_else(|e| {
                eprintln!("store: {e}");
                std::process::exit(1)
            });
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .unwrap_or_else(|e| {
                    eprintln!("runtime: {e}");
                    std::process::exit(1)
                });
            match runtime.block_on(swb_broker::serve(
                std::sync::Arc::new(store),
                &listen,
                &metrics,
            )) {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("serve: {e}");
                    ExitCode::FAILURE
                }
            }
        }
        Some("agentd") => {
            eprintln!("swb agentd: planned for P2");
            ExitCode::from(3)
        }
        _ => {
            eprintln!(
                "usage: swb <serve|agentd|hook <harness> <event>|whoami|inbox|send|ack|peers|doctor|version>"
            );
            ExitCode::from(2)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    #[test]
    fn explicit_broker_round_trip_and_session_identity() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut request_bytes = Vec::new();
            loop {
                let mut chunk = [0u8; 4096];
                let n = stream.read(&mut chunk).unwrap();
                assert!(n > 0, "client closed before request body");
                request_bytes.extend_from_slice(&chunk[..n]);
                if let Some(split) = request_bytes.windows(4).position(|b| b == b"\r\n\r\n") {
                    let header = String::from_utf8_lossy(&request_bytes[..split]);
                    let length: usize = header
                        .lines()
                        .find_map(|line| line.strip_prefix("Content-Length: "))
                        .unwrap()
                        .parse()
                        .unwrap();
                    if request_bytes.len() >= split + 4 + length {
                        break;
                    }
                }
            }
            let request = String::from_utf8_lossy(&request_bytes);
            assert!(request.starts_with("POST /v1/register HTTP/1.1"));
            assert!(request.contains("\"session_id\":\"session-1\""));
            assert!(request.contains("\"proc_start\":\"start-1\""));
            let body = r#"{"agent_id":"pi:honey:123:session-1","lease_seconds":900}"#;
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
        });
        // One test owns these process-wide variables; no other test in this
        // binary mutates them. The broker URL is intentionally loopback only.
        unsafe {
            std::env::set_var("SWB_BROKER_URL", format!("http://127.0.0.1:{port}"));
            std::env::set_var("SWB_HOST", "honey");
            std::env::set_var("SWB_SESSION_PID", "123");
            std::env::set_var("SWB_PROC_START", "start-1");
        }
        let result = register("pi", "session-1").unwrap();
        assert_eq!(result["agent_id"], "pi:honey:123:session-1");
        server.join().unwrap();
    }

    #[test]
    fn inbox_rejects_request_path_injection() {
        assert_eq!(
            inbox("pi:honey:1:session?limit=1000").unwrap_err(),
            "invalid SWB_AGENT_ID"
        );
    }

    #[test]
    fn one_maximum_escaped_envelope_fits_cli_page() {
        let store = swb_store::Store::memory().unwrap();
        let to = "pi:sting:2:b";
        let artifacts: Vec<String> = (0..20)
            .map(|i| format!("{}{:02}", "\u{0000}".repeat(2046), i))
            .collect();
        let sent = store
            .send(&json!({
                "from":"claude:honey:1:a","to":to,"ticket":"TIN-4655",
                "body":"\"".repeat(16384),"artifacts":artifacts
            }))
            .unwrap();
        assert_eq!(sent.envelope["body"].as_str().unwrap().len(), 16384);
        let response = store.inbox(to, CLI_INBOX_PAGE).unwrap().to_string();
        assert!(
            response.len() > 128 * 1024,
            "regression must exceed old cap"
        );
        assert!(
            response.len() < MAX_RESPONSE_BYTES,
            "one valid escaped page must fit"
        );
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = Vec::new();
            loop {
                let mut chunk = [0u8; 512];
                let n = stream.read(&mut chunk).unwrap();
                assert!(n > 0);
                request.extend_from_slice(&chunk[..n]);
                if request.windows(4).any(|bytes| bytes == b"\r\n\r\n") {
                    break;
                }
            }
            assert!(String::from_utf8_lossy(&request).contains("limit=1&wait_seconds=0"));
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}",
                response.len()
            )
            .unwrap();
        });
        let received = blocking_call(
            "127.0.0.1",
            port,
            "GET",
            &format!("/v1/inbox?me={to}&limit={CLI_INBOX_PAGE}&wait_seconds=0"),
            None,
        )
        .unwrap();
        assert_eq!(
            received["messages"][0]["body"].as_str().unwrap().len(),
            16384
        );
        server.join().unwrap();
    }

    #[test]
    fn stalled_resolution_worker_cannot_hold_caller() {
        let start = Instant::now();
        let error = bounded(Duration::from_millis(75), || {
            // A deliberately slow resolver substitute has the same blocking
            // behavior as to_socket_addrs; the worker is never signaled.
            std::thread::sleep(Duration::from_millis(700));
            Ok::<_, String>(())
        })
        .unwrap_err();
        assert_eq!(error, "broker deadline exceeded");
        assert!(start.elapsed() < Duration::from_millis(500));
    }

    #[test]
    fn stalled_http_read_cannot_hold_caller() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            let (_stream, _) = listener.accept().unwrap();
            std::thread::sleep(Duration::from_millis(700));
        });
        let start = Instant::now();
        let error = bounded(Duration::from_millis(75), move || {
            blocking_call("127.0.0.1", port, "GET", "/v1/peers", None)
        })
        .unwrap_err();
        assert_eq!(error, "broker deadline exceeded");
        assert!(start.elapsed() < Duration::from_millis(500));
        server.join().unwrap();
    }

    // ---- R-C275 verbs. These tests inject SWB_* through a closure and never
    // touch process environment, so they cannot race the test above.

    const ME: &str = "claude:neo:41:s-1";
    const PEER: &str = "codex:sting:42:t-1";
    const ULID: &str = "01J9ZQ3Y8T6V2W4X5Y6Z7A8B9C";

    fn lookup_of(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> + use<> {
        let map: std::collections::HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect();
        move |name: &str| map.get(name).cloned()
    }

    /// Serves `replies` in order, one connection each, and returns every
    /// request it read (head and body) once all replies are sent.
    fn mock(
        replies: Vec<(u16, &'static str, &'static str)>,
    ) -> (String, std::thread::JoinHandle<Vec<String>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
        let server = std::thread::spawn(move || {
            let mut seen = Vec::new();
            for (status, content_type, body) in replies {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut request = Vec::new();
                loop {
                    let mut chunk = [0u8; 4096];
                    let n = stream.read(&mut chunk).unwrap();
                    assert!(n > 0, "client closed early");
                    request.extend_from_slice(&chunk[..n]);
                    if let Some(split) = request.windows(4).position(|b| b == b"\r\n\r\n") {
                        let head = String::from_utf8_lossy(&request[..split]).to_string();
                        let length: usize = head
                            .lines()
                            .find_map(|l| l.strip_prefix("Content-Length: "))
                            .unwrap()
                            .parse()
                            .unwrap();
                        if request.len() >= split + 4 + length {
                            break;
                        }
                    }
                }
                seen.push(String::from_utf8_lossy(&request).into_owned());
                write!(
                    stream,
                    "HTTP/1.1 {status} X\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )
                .unwrap();
            }
            seen
        });
        (url, server)
    }

    fn request_json(request: &str) -> Value {
        let split = request.find("\r\n\r\n").unwrap();
        serde_json::from_str(&request[split + 4..]).unwrap()
    }

    fn strings(args: &[&str]) -> Vec<String> {
        args.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn send_args_parse_flags_and_positionals() {
        let parsed = parse_send_args(&strings(&[
            PEER,
            "TIN-4655",
            "--ruling",
            "R-C275",
            "--operator-directed",
            "--thread",
            ULID,
        ]))
        .unwrap();
        assert_eq!(parsed.to, PEER);
        assert_eq!(parsed.ticket, "TIN-4655");
        assert_eq!(parsed.ruling.as_deref(), Some("R-C275"));
        assert!(parsed.operator_directed);
        assert_eq!(parsed.thread_id.as_deref(), Some(ULID));
        assert_eq!(
            parse_send_args(&strings(&[PEER])).unwrap_err(),
            "expected <to-agent-id> <ticket>"
        );
        assert_eq!(
            parse_send_args(&strings(&[PEER, "none", "--bogus"])).unwrap_err(),
            "unknown option --bogus"
        );
        assert_eq!(
            parse_send_args(&strings(&[PEER, "none", "--ruling"])).unwrap_err(),
            "--ruling needs a value"
        );
    }

    #[test]
    fn send_payload_validates_like_the_broker() {
        let args = |to: &str, ticket: &str| SendArgs {
            to: to.into(),
            ticket: ticket.into(),
            ..SendArgs::default()
        };
        let ok = send_payload(ME, &args(PEER, "TIN-1"), "hi").unwrap();
        assert_eq!(
            ok,
            json!({"from":ME,"to":PEER,"ticket":"TIN-1","body":"hi"})
        );
        assert!(send_payload(ME, &args("pi:x", "none"), "hi").is_err());
        assert!(send_payload(ME, &args("bash:h:1:s", "none"), "hi").is_err());
        for ticket in ["TIN-", "TIN-12a", "LAB-1", ""] {
            assert!(
                send_payload(ME, &args(PEER, ticket), "hi").is_err(),
                "{ticket}"
            );
        }
        assert!(send_payload(ME, &args(PEER, "none"), "").is_err());
        assert!(send_payload(ME, &args(PEER, "none"), &"x".repeat(16385)).is_err());
        let directed = SendArgs {
            operator_directed: true,
            ..args(PEER, "none")
        };
        assert_eq!(
            send_payload(ME, &directed, "hi").unwrap_err(),
            "--operator-directed requires --ruling"
        );
        let directed = SendArgs {
            ruling: Some("R-C275".into()),
            ..directed
        };
        let payload = send_payload(ME, &directed, "hi").unwrap();
        assert_eq!(payload["operator_directed"], true);
        assert_eq!(payload["ruling"], "R-C275");
        let bad_reply = SendArgs {
            in_reply_to: Some("01j9zq3y8t6v2w4x5y6z7a8b9c".into()),
            ..args(PEER, "none")
        };
        assert_eq!(
            send_payload(ME, &bad_reply, "hi").unwrap_err(),
            "invalid in_reply_to (ULID)"
        );
    }

    #[test]
    fn body_is_bounded_utf8_from_stdin() {
        assert_eq!(read_body(&b"hello\n"[..]).unwrap(), "hello\n");
        assert_eq!(
            read_body(&vec![b'a'; 16385][..]).unwrap_err(),
            "body exceeds 16 KiB"
        );
        assert_eq!(
            read_body(&[0xffu8, 0xfe][..]).unwrap_err(),
            "body is not UTF-8"
        );
    }

    #[test]
    fn send_posts_envelope_as_swb_agent_id() {
        let (url, server) = mock(vec![(
            200,
            "application/json",
            r#"{"msg_id":"01J9ZQ3Y8T6V2W4X5Y6Z7A8B9C","authority":"peer"}"#,
        )]);
        let lookup = lookup_of(&[("SWB_BROKER_URL", &url), ("SWB_AGENT_ID", ME)]);
        let args = parse_send_args(&strings(&[PEER, "TIN-4655"])).unwrap();
        let result = send_cmd(&lookup, &args, "status: green").unwrap();
        assert_eq!(result["authority"], "peer");
        let requests = server.join().unwrap();
        assert!(requests[0].starts_with("POST /v1/send HTTP/1.1"));
        let sent = request_json(&requests[0]);
        assert_eq!(sent["from"], ME);
        assert_eq!(sent["to"], PEER);
        assert_eq!(sent["body"], "status: green");
        let no_me = lookup_of(&[("SWB_BROKER_URL", &url)]);
        assert_eq!(
            send_cmd(&no_me, &args, "x").unwrap_err(),
            "SWB_AGENT_ID is required"
        );
    }

    #[test]
    fn ack_posts_me_and_msg_id_and_surfaces_broker_error() {
        let (url, server) = mock(vec![
            (
                200,
                "application/json",
                r#"{"msg_id":"01J9ZQ3Y8T6V2W4X5Y6Z7A8B9C","state":"acked"}"#,
            ),
            (
                400,
                "application/json",
                r#"{"error":"message unavailable"}"#,
            ),
        ]);
        let lookup = lookup_of(&[("SWB_BROKER_URL", &url), ("SWB_AGENT_ID", ME)]);
        assert_eq!(ack_cmd(&lookup, ULID).unwrap()["state"], "acked");
        assert_eq!(
            ack_cmd(&lookup, ULID).unwrap_err(),
            "broker HTTP 400: message unavailable"
        );
        let requests = server.join().unwrap();
        assert!(requests[0].starts_with("POST /v1/ack HTTP/1.1"));
        assert_eq!(request_json(&requests[0]), json!({"me":ME,"msg_id":ULID}));
        assert_eq!(
            ack_cmd(&lookup, "not-a-ulid").unwrap_err(),
            "invalid msg_id (ULID)"
        );
    }

    #[test]
    fn peers_is_a_read_only_listing() {
        let (url, server) = mock(vec![(
            200,
            "application/json",
            r#"{"peers":[{"agent_id":"codex:sting:42:t-1","state":"live"}]}"#,
        )]);
        let lookup = lookup_of(&[("SWB_BROKER_URL", &url), ("SWB_AGENT_ID", ME)]);
        let result = peers_cmd(&lookup).unwrap();
        assert_eq!(result["peers"][0]["agent_id"], PEER);
        let requests = server.join().unwrap();
        // No `me`: listing never refreshes a lease or needs registration.
        assert!(requests[0].starts_with("GET /v1/peers HTTP/1.1"));
    }

    #[test]
    fn doctor_with_nothing_set_fails_with_fixes() {
        let checks = doctor_checks(&lookup_of(&[]), true);
        let mut out = Vec::new();
        assert!(!render_doctor(&checks, &mut out));
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("FAIL  env SWB_BROKER_URL: SWB_BROKER_URL is unset"));
        assert!(text.contains("FAIL  env identity: unset: SWB_HARNESS, SWB_HOST, SWB_SESSION_PID, SWB_PROC_START, SWB_SESSION_ID"));
        assert!(text.contains("fix: the lab harness launcher exports these (R-C273)"));
        assert!(text.contains("skip  broker REST"));
        assert!(text.contains("skip  register: identity incomplete"));
    }

    #[test]
    fn doctor_rejects_invalid_values() {
        let lookup = lookup_of(&[
            ("SWB_BROKER_URL", "https://broker:8080/v1"),
            ("SWB_HARNESS", "bash"),
            ("SWB_HOST", "neo"),
            ("SWB_SESSION_PID", "41"),
            ("SWB_PROC_START", "start"),
            ("SWB_SESSION_ID", "s-1"),
            ("SWB_AGENT_ID", "nope"),
        ]);
        let checks = doctor_checks(&lookup, false);
        assert!(
            matches!(&checks[0].1, Check::Fail { detail, .. } if detail == "SWB_BROKER_URL must be http://")
        );
        assert!(
            matches!(&checks[1].1, Check::Fail { detail, .. } if detail.contains("not a known harness"))
        );
        assert!(matches!(&checks[2].1, Check::Fail { .. }));
        let pid0 = lookup_of(&[
            ("SWB_HARNESS", "claude"),
            ("SWB_HOST", "neo"),
            ("SWB_SESSION_PID", "0"),
            ("SWB_PROC_START", "start"),
            ("SWB_SESSION_ID", "s-1"),
        ]);
        assert!(
            matches!(&doctor_checks(&pid0, false)[1].1, Check::Fail { detail, .. } if detail.contains("SWB_SESSION_PID"))
        );
    }

    fn identity_env(url: &str) -> Vec<(&'static str, String)> {
        vec![
            ("SWB_BROKER_URL", url.to_owned()),
            ("SWB_HARNESS", "claude".into()),
            ("SWB_HOST", "neo".into()),
            ("SWB_SESSION_PID", "41".into()),
            ("SWB_PROC_START", "start-41".into()),
            ("SWB_SESSION_ID", "s-1".into()),
        ]
    }

    #[test]
    fn doctor_dry_run_makes_no_register_request() {
        let (url, server) = mock(vec![(200, "application/json", r#"{"peers":[]}"#)]);
        let mut pairs = identity_env(&url);
        pairs.push(("SWB_AGENT_ID", "claude:neo:41:other".into()));
        let borrowed: Vec<(&str, &str)> = pairs.iter().map(|(k, v)| (*k, v.as_str())).collect();
        let checks = doctor_checks(&lookup_of(&borrowed), false);
        let mut out = Vec::new();
        assert!(!render_doctor(&checks, &mut out), "agent id mismatch fails");
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("ok    env identity: would register as claude:neo:41:s-1"));
        assert!(text.contains("FAIL  env SWB_AGENT_ID: claude:neo:41:other differs"));
        assert!(text.contains("ok    broker REST: GET /v1/peers listed 0 session(s)"));
        assert!(text.contains("ok    register: dry run: payload valid for claude:neo:41:s-1"));
        let requests = server.join().unwrap();
        assert_eq!(requests.len(), 1);
        assert!(requests[0].starts_with("GET /v1/peers "));
    }

    #[test]
    fn doctor_live_register_round_trip_and_metrics() {
        let (url, server) = mock(vec![
            (
                200,
                "application/json",
                r#"{"peers":[{"agent_id":"codex:sting:42:t-1"}]}"#,
            ),
            (
                200,
                "application/json",
                r#"{"agent_id":"claude:neo:41:s-1","lease_seconds":900}"#,
            ),
        ]);
        let (metrics_url, metrics) = mock(vec![(
            200,
            "text/plain; version=0.0.4",
            "# TYPE swb_sessions gauge\nswb_sessions{state=\"live\"} 1\n",
        )]);
        let mut pairs = identity_env(&url);
        pairs.push(("SWB_AGENT_ID", ME.into()));
        pairs.push(("SWB_METRICS_URL", metrics_url));
        let borrowed: Vec<(&str, &str)> = pairs.iter().map(|(k, v)| (*k, v.as_str())).collect();
        let checks = doctor_checks(&lookup_of(&borrowed), true);
        let mut out = Vec::new();
        let healthy = render_doctor(&checks, &mut out);
        let text = String::from_utf8(out).unwrap();
        assert!(healthy, "{text}");
        assert!(text.contains("ok    broker /metrics: GET /metrics exposes swb_* series"));
        assert!(text.contains("ok    register: POST /v1/register returned claude:neo:41:s-1"));
        let requests = server.join().unwrap();
        assert!(requests[1].starts_with("POST /v1/register "));
        assert_eq!(
            request_json(&requests[1]),
            json!({"harness":"claude","host":"neo","pid":41,"session_id":"s-1","proc_start":"start-41"})
        );
        assert!(metrics.join().unwrap()[0].starts_with("GET /metrics "));
    }

    #[test]
    fn doctor_reports_unreachable_broker() {
        // Bind then drop: nothing listens on this port.
        let port = TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let url = format!("http://127.0.0.1:{port}");
        let pairs = identity_env(&url);
        let borrowed: Vec<(&str, &str)> = pairs.iter().map(|(k, v)| (*k, v.as_str())).collect();
        let checks = doctor_checks(&lookup_of(&borrowed), true);
        assert!(
            matches!(&checks[3].1, Check::Fail { detail, .. } if detail.starts_with("GET /v1/peers: "))
        );
        assert!(
            matches!(&checks[5].1, Check::Fail { detail, .. } if detail.starts_with("POST /v1/register: "))
        );
    }
}
