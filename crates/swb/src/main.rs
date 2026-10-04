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
    std::env::var(name)
        .ok()
        .filter(|v| !v.is_empty())
        .ok_or_else(|| format!("{name} is required"))
}

fn endpoint() -> Result<(String, u16), String> {
    let url = env("SWB_BROKER_URL")?;
    let rest = url
        .strip_prefix("http://")
        .ok_or("SWB_BROKER_URL must be http://")?;
    if rest.contains(['/', '@', '?']) {
        return Err("broker URL must contain host and port only".into());
    }
    let (host, port) = rest.rsplit_once(':').ok_or("broker URL needs a port")?;
    if host.is_empty() || host.contains(':') {
        return Err("invalid broker host".into());
    }
    Ok((
        host.into(),
        port.parse().map_err(|_| "invalid broker port")?,
    ))
}

fn call(method: &str, path: &str, data: Option<&Value>) -> Result<Value, String> {
    let (host, port) = endpoint()?;
    let method = method.to_owned();
    let path = path.to_owned();
    let data = data.cloned();
    // DNS and all socket operations run in a process-local worker. The
    // caller's deadline does not depend on resolver cancellation or EOF.
    bounded(LIMIT, move || {
        blocking_call(&host, port, &method, &path, data.as_ref())
    })
}

fn blocking_call(
    host: &str,
    port: u16,
    method: &str,
    path: &str,
    data: Option<&Value>,
) -> Result<Value, String> {
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
    let result: Value = serde_json::from_slice(&bytes[split + 4..]).map_err(|e| e.to_string())?;
    if !(200..300).contains(&status) {
        return Err(format!("broker HTTP {status}"));
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
            #[cfg(feature = "test-clock")]
            if std::env::var("SWB_TEST_CLOCK").as_deref() == Ok("1") {
                return serve_test_clock(&path, &listen, &metrics);
            }
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
            eprintln!("usage: swb <serve|agentd|hook <harness> <event>|whoami|inbox|version>");
            ExitCode::from(2)
        }
    }
}

/// R-C262: `swb serve` on a manual clock that only `POST /v1/test/clock`
/// moves, for the spec live adapter. Compiled only with the `test-clock`
/// feature and used only when `SWB_TEST_CLOCK=1`; both listeners must be
/// loopback.
#[cfg(feature = "test-clock")]
fn serve_test_clock(path: &str, listen: &str, metrics: &str) -> ExitCode {
    let clock = std::sync::Arc::new(swb_store::ManualClock::starting_now());
    let store = swb_store::Store::open_with_clock(path, clock.clone()).unwrap_or_else(|e| {
        eprintln!("store: {e}");
        std::process::exit(1)
    });
    eprintln!("swb serve: SWB_TEST_CLOCK=1, manual clock at {}", store.now());
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap_or_else(|e| {
            eprintln!("runtime: {e}");
            std::process::exit(1)
        });
    match runtime.block_on(swb_broker::test_clock::serve(
        std::sync::Arc::new(store),
        clock,
        listen,
        metrics,
    )) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("serve: {e}");
            ExitCode::FAILURE
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
}
