//! Tailnet broker REST and rmcp Streamable HTTP server (SWB-R02, SWB-R14,
//! SWB-R33, SWB-R46). Audit output omits bodies until SWB-R33/34 proof.
//! Every route on the main listener passes the Host/Origin guard
//! ([`RequestGuard`], ADR-0001 → Exposure and auth).
//!
//! v0.3.0 (R-C944): every state-changing call is a POST with a JSON body
//! (`GET /v1/inbox` and `GET /v1/peers?me=` answer 405), `end` needs the
//! end token `register` returned, errors are typed JSON
//! (`{"error": <message>, "code": <code>}`), and `/healthz` answers on both
//! listeners.

use axum::{
    Json, Router,
    extract::{Query, Request, State, rejection::JsonRejection},
    http::{HeaderMap, HeaderValue, StatusCode, Uri, header, uri::Authority as UriAuthority},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use rmcp::{
    ErrorData as McpError, RoleServer, ServerHandler,
    model::{
        CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, Implementation,
        ListToolsResult, PaginatedRequestParams, ServerCapabilities, ServerConfig, Tool,
    },
    service::RequestContext,
    transport::streamable_http_server::{
        StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
    },
};
use serde_json::{Value, json};
use std::{collections::HashMap, future::Future, io, sync::Arc, time::Duration};
use swb_proto::{Authority, VERSION};
use swb_store::{ErrorKind, PeersQuery, Store, StoreError};

pub const HOOK_TIMEOUT_MS: u64 = 2_000;
/// Production retention interval. The store no longer prunes inside `send`
/// or the 250 ms inbox long-poll (R-C944).
pub const PRUNE_INTERVAL: Duration = Duration::from_secs(3600);
pub fn stamped_authority() -> Authority {
    Authority::Peer
}

/// An optional field that must have one JSON type when present.
fn opt<'a, T>(
    args: &'a Value,
    key: &str,
    read: impl Fn(&'a Value) -> Option<T>,
) -> Result<Option<T>, StoreError> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => read(value)
            .map(Some)
            .ok_or_else(|| format!("invalid {key}").into()),
    }
}

fn peers_op(store: &Store, args: &Value) -> Result<Value, StoreError> {
    let limit = opt(args, "limit", Value::as_u64)?;
    store.peers(&PeersQuery {
        me: opt(args, "me", Value::as_str)?,
        all: opt(args, "all", Value::as_bool)?.unwrap_or(false),
        host: opt(args, "host", Value::as_str)?,
        limit: limit.map(|n| n.min(u64::from(u32::MAX)) as u32),
        cursor: opt(args, "cursor", Value::as_str)?,
        proc_start: opt(args, "proc_start", Value::as_str)?,
    })
}

fn operation(store: &Store, name: &str, args: &Value) -> Result<Value, StoreError> {
    let mut created = true;
    let result = match name {
        "register" => store.register(args),
        "end" => {
            if args.get("end_token").is_none() && args.get("proc_start").is_some() {
                return Err(
                    "end needs end_token; proc_start is no longer accepted (v0.3.0)".into(),
                );
            }
            store.end(
                args.get("me").and_then(Value::as_str).ok_or("missing me")?,
                args.get("end_token")
                    .and_then(Value::as_str)
                    .ok_or("missing end_token")?,
            )
        }
        "peers" => peers_op(store, args),
        "send" => {
            let outcome = store.send(args)?;
            created = outcome.created;
            Ok(outcome.envelope)
        }
        "inbox" => {
            let me = args.get("me").and_then(Value::as_str).ok_or("missing me")?;
            let limit = opt(args, "limit", Value::as_u64)?
                .unwrap_or(20)
                .clamp(1, 100) as u32;
            let wait = opt(args, "wait_seconds", Value::as_u64)?
                .unwrap_or(0)
                .min(25);
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(wait);
            loop {
                let batch = store.inbox(me, limit)?;
                if batch["messages"].as_array().is_some_and(|v| !v.is_empty())
                    || std::time::Instant::now() >= deadline
                {
                    break Ok(batch);
                }
                std::thread::sleep(std::time::Duration::from_millis(250));
            }
        }
        "ack" => store.ack(
            args.get("me").and_then(Value::as_str).ok_or("missing me")?,
            args.get("msg_id")
                .and_then(Value::as_str)
                .ok_or("missing msg_id")?,
        ),
        _ => Err("unknown operation".into()),
    }?;
    if created && matches!(name, "register" | "send" | "ack") {
        let body = result.get("body").and_then(Value::as_str).unwrap_or("");
        let body_hmac = if name == "send" {
            Some(store.body_hmac(body)?)
        } else {
            None
        };
        println!(
            "{}",
            audit_record(name, args, &result, body_hmac.as_deref())
        );
    }
    Ok(result)
}

/// The audit line. It names fields explicitly, so neither the message body
/// nor a register's `end_token` can reach it.
fn audit_record(name: &str, args: &Value, result: &Value, body_hmac: Option<&str>) -> Value {
    let body_size = result
        .get("body")
        .and_then(Value::as_str)
        .map_or(0, str::len);
    json!({
        "ts":std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0,|d|d.as_secs()),
        "op":name,"me":args.get("me").or_else(||args.get("from")),"to":result.get("to"),
        "ticket":result.get("ticket"),"ruling":result.get("ruling"),
        "msg_id":result.get("msg_id"),"size":body_size,"body_hmac":body_hmac,
        "agent_id":result.get("agent_id"),"state":result.get("state")
    })
}

/// The typed error body every REST and MCP failure carries (R-C944).
pub fn error_body(code: &str, message: &str) -> Value {
    json!({"error": message, "code": code})
}

fn status_of(kind: ErrorKind) -> StatusCode {
    match kind {
        ErrorKind::Invalid => StatusCode::BAD_REQUEST,
        ErrorKind::Unregistered | ErrorKind::Unavailable => StatusCode::NOT_FOUND,
        ErrorKind::Conflict => StatusCode::CONFLICT,
        ErrorKind::Internal => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

const AGENT_ID: &str = "Agent id <harness>:<host>:<pid>:<session_id>; harness is one of claude, kimi, codex, junie, opencode, pi; the other parts use [A-Za-z0-9._-]; at most 512 bytes";
const ULID_DESC: &str = "Canonical 26-character Crockford base32 ULID";

fn tools() -> Vec<Tool> {
    [
        ("register", "Register (or re-register) a self-asserted agent session. Returns agent_id, lease_seconds and end_token. Every register mints a new end_token and invalidates earlier ones. Over MCP the token is part of this result and so enters the caller's context; an adapter that ends sessions itself should keep it and strip it before the model sees it. Never repeat it.", json!({"type":"object","required":["harness","host","pid","session_id","proc_start"],"properties":{
            "harness":{"type":"string","enum":["claude","kimi","codex","junie","opencode","pi"],"description":"The harness kind"},
            "host":{"type":"string","pattern":"^[A-Za-z0-9._-]+$","description":"Short host name"},
            "pid":{"type":"integer","minimum":1,"description":"The harness launcher's process id"},
            "session_id":{"type":"string","pattern":"^[A-Za-z0-9._-]+$","description":"The harness's own session id"},
            "proc_start":{"type":"string","minLength":1,"maxLength":256,"description":"Opaque process start stamp. peers never returns it; a caller may filter peers on its own."}}})),
        ("end", "End this session. Needs the end_token its latest register returned; any other token answers state unknown and ends nothing.", json!({"type":"object","required":["me","end_token"],"properties":{
            "me":{"type":"string","description":AGENT_ID},
            "end_token":{"type":"string","pattern":"^[0-9a-f]{64}$","description":"The end_token register returned"}}})),
        ("peers", "List sessions: live only by default, one row per harness, host and session id (latest registration wins), paginated. A registered me also renews that caller's own lease; omitting me is passive discovery.", json!({"type":"object","properties":{
            "me":{"type":"string","description":format!("{AGENT_ID}. Optional; renews only this caller's lease")},
            "all":{"type":"boolean","description":"Include idle, gone and ended sessions (default false: live only)"},
            "host":{"type":"string","pattern":"^[A-Za-z0-9._-]+$","description":"Only sessions on this host"},
            "limit":{"type":"integer","minimum":1,"maximum":200,"description":"Page size, default 50, at most 200"},
            "cursor":{"type":"string","description":"next_cursor from the previous page"},
            "proc_start":{"type":"string","minLength":1,"maxLength":256,"description":"Only sessions whose latest registration used this proc_start (the caller's own stamp; rows never show it)"}}})),
        ("send", "Send a peer-authority message. The broker stamps authority peer whatever the sender claims.", json!({"type":"object","required":["from","to","ticket","body"],"properties":{
            "from":{"type":"string","description":format!("Sender. {AGENT_ID}")},
            "to":{"type":"string","description":format!("Recipient. {AGENT_ID}")},
            "ticket":{"type":"string","pattern":"^(TIN-[0-9]+|none)$","maxLength":64,"description":"TIN-<digits> or none"},
            "body":{"type":"string","maxLength":16384,"description":"Message text, at most 16 KiB; control characters are removed"},
            "msg_id":{"type":"string","description":format!("{ULID_DESC}. Optional idempotency key; a retry with the same sender and body returns the stored envelope")},
            "thread_id":{"type":"string","description":format!("{ULID_DESC}. Defaults to msg_id")},
            "in_reply_to":{"type":"string","description":format!("{ULID_DESC} of the message answered")},
            "operator_directed":{"type":"boolean","description":"The sender's claim that the operator directed this; needs ruling"},
            "ruling":{"type":"string","minLength":1,"maxLength":512,"description":"Ruling the sender cites"},
            "reply_expires":{"type":"string","format":"date-time","description":"RFC 3339 time after which a reply is no longer useful"},
            "reply_format":{"type":"string","maxLength":512,"description":"Requested reply shape"},
            "artifacts":{"type":"array","maxItems":20,"uniqueItems":true,"items":{"type":"string","minLength":1,"maxLength":2048},"description":"Up to 20 artifact references"},
            "ttl_hours":{"type":"integer","minimum":1,"maximum":336,"description":"Message lifetime, default 72 h, clamped to 1..336"}}})),
        ("inbox", "Fetch this caller's unacknowledged messages, oldest first. Each fetch counts a delivery and renews the caller's lease.", json!({"type":"object","required":["me"],"properties":{
            "me":{"type":"string","description":AGENT_ID},
            "limit":{"type":"integer","minimum":1,"maximum":100,"description":"Page size, default 20, at most 100"},
            "wait_seconds":{"type":"integer","minimum":0,"maximum":25,"description":"Long-poll up to this many seconds when the mailbox is empty (default 0)"}}})),
        ("ack", "Acknowledge a received message so it is not delivered again.", json!({"type":"object","required":["me","msg_id"],"properties":{
            "me":{"type":"string","description":AGENT_ID},
            "msg_id":{"type":"string","description":ULID_DESC}}})),
    ].into_iter().map(|(name,description,schema)| Tool::new(name,description,Arc::new(schema.as_object().expect("static object schema").clone()))).collect()
}

const TOOL_NAMES: [&str; 6] = ["register", "end", "peers", "send", "inbox", "ack"];

#[derive(Clone)]
struct BrokerMcp {
    store: Arc<Store>,
}
impl ServerHandler for BrokerMcp {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("agent-switchboard", VERSION))
    }
    async fn list_tools(
        &self,
        _: Option<PaginatedRequestParams>,
        _: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        Ok(ListToolsResult {
            tools: tools(),
            ..Default::default()
        })
    }
    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, McpError> {
        let name = request.name.into_owned();
        if !TOOL_NAMES.contains(&name.as_str()) {
            return Err(McpError::invalid_params("unknown tool", None));
        }
        let args = Value::Object(request.arguments.unwrap_or_default());
        let store = Arc::clone(&self.store);
        match tokio::task::spawn_blocking(move || operation(&store, &name, &args)).await {
            Ok(Ok(value)) => Ok(CallToolResult::structured(value).into()),
            Ok(Err(error)) => Ok(CallToolResult::error(vec![ContentBlock::text(
                error_body(error.code(), &error.message).to_string(),
            )])
            .into()),
            Err(error) => Err(McpError::internal_error(error.to_string(), None)),
        }
    }
}

fn error_response(status: StatusCode, code: &str, message: &str) -> Response {
    (status, Json(error_body(code, message))).into_response()
}

async fn run_operation(store: Arc<Store>, name: &'static str, args: Value) -> Response {
    match tokio::task::spawn_blocking(move || operation(&store, name, &args)).await {
        Ok(Ok(value)) => (StatusCode::OK, Json(value)).into_response(),
        Ok(Err(error)) => error_response(status_of(error.kind), error.code(), &error.message),
        Err(_) => error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal",
            "broker unavailable",
        ),
    }
}

/// A JSON request body, or the typed error for a missing, mistyped or
/// malformed one. A JSON body needs `Content-Type: application/json`, which
/// a browser can only send after a CORS preflight.
fn json_body(body: Result<Json<Value>, JsonRejection>) -> Result<Value, Response> {
    match body {
        Ok(Json(value @ Value::Object(_))) => Ok(value),
        Ok(Json(_)) => Err(error_response(
            StatusCode::BAD_REQUEST,
            "invalid_json",
            "request body must be a JSON object",
        )),
        Err(JsonRejection::MissingJsonContentType(_)) => Err(error_response(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "unsupported_media_type",
            "Content-Type must be application/json",
        )),
        Err(_) => Err(error_response(
            StatusCode::BAD_REQUEST,
            "invalid_json",
            "request body is not valid JSON",
        )),
    }
}

macro_rules! post_operation {
    ($handler:ident, $name:literal) => {
        async fn $handler(
            State(store): State<Arc<Store>>,
            body: Result<Json<Value>, JsonRejection>,
        ) -> Response {
            match json_body(body) {
                Ok(args) => run_operation(store, $name, args).await,
                Err(response) => response,
            }
        }
    };
}
post_operation!(register, "register");
post_operation!(end, "end");
post_operation!(send, "send");
post_operation!(ack, "ack");
post_operation!(peers_post, "peers");
post_operation!(inbox_post, "inbox");

fn method_not_allowed(allow: &'static str, message: &str) -> Response {
    let mut response = error_response(
        StatusCode::METHOD_NOT_ALLOWED,
        "method_not_allowed",
        message,
    );
    response
        .headers_mut()
        .insert(header::ALLOW, HeaderValue::from_static(allow));
    response
}

/// `GET /v1/peers` is passive discovery only (`all`, `host`, `limit`,
/// `cursor`). A lease refresh (`me`) moved to `POST /v1/peers` in v0.3.0,
/// and is refused here rather than silently ignored.
async fn peers_get(
    State(store): State<Arc<Store>>,
    Query(query): Query<HashMap<String, String>>,
) -> Response {
    if query.contains_key("me") {
        return method_not_allowed(
            "POST",
            "lease refresh moved to POST /v1/peers with a JSON body {\"me\": ...} in v0.3.0",
        );
    }
    let mut args = serde_json::Map::new();
    for (key, value) in &query {
        let parsed = match key.as_str() {
            "all" => match value.as_str() {
                "1" | "true" => json!(true),
                "0" | "false" => json!(false),
                _ => {
                    return error_response(
                        StatusCode::BAD_REQUEST,
                        "invalid_argument",
                        "invalid all",
                    );
                }
            },
            "limit" => match value.parse::<u64>() {
                Ok(n) => json!(n),
                Err(_) => {
                    return error_response(
                        StatusCode::BAD_REQUEST,
                        "invalid_argument",
                        "invalid limit",
                    );
                }
            },
            "host" | "cursor" => json!(value),
            _ => {
                return error_response(
                    StatusCode::BAD_REQUEST,
                    "invalid_argument",
                    &format!(
                        "unknown query parameter {}",
                        key.chars()
                            .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
                            .take(32)
                            .collect::<String>()
                    ),
                );
            }
        };
        args.insert(key.clone(), parsed);
    }
    run_operation(store, "peers", Value::Object(args)).await
}

async fn inbox_get() -> Response {
    method_not_allowed(
        "POST",
        "GET /v1/inbox was removed in v0.3.0; POST /v1/inbox with a JSON body {\"me\": ..., \"limit\": ..., \"wait_seconds\": ...}",
    )
}

async fn not_found() -> Response {
    error_response(StatusCode::NOT_FOUND, "not_found", "no such route")
}

async fn wrong_method() -> Response {
    error_response(
        StatusCode::METHOD_NOT_ALLOWED,
        "method_not_allowed",
        "method not allowed on this route",
    )
}

/// `GET /healthz`: 200 with the version when the store answers a read, 503
/// otherwise. No identity, no body, no write.
async fn healthz(State(store): State<Arc<Store>>) -> Response {
    match tokio::task::spawn_blocking(move || store.health()).await {
        Ok(Ok(())) => (
            StatusCode::OK,
            Json(json!({"status":"ok","version":VERSION})),
        )
            .into_response(),
        _ => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({"status":"unavailable","version":VERSION})),
        )
            .into_response(),
    }
}

async fn metrics(State(store): State<Arc<Store>>) -> Response {
    match tokio::task::spawn_blocking(move || store.metrics()).await {
        Ok(Ok(value)) => (
            StatusCode::OK,
            [(
                axum::http::header::CONTENT_TYPE,
                "text/plain; version=0.0.4",
            )],
            value,
        )
            .into_response(),
        _ => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

/// The metrics listener's router. It carries no Host/Origin guard: it serves
/// three aggregate counters and `/healthz`, with no identity, body or write
/// path, it is a separate listener (`SWB_METRICS_LISTEN`) that a deployment
/// keeps off the tailnet Service, and Prometheus and kubelet probes reach it
/// by pod address, a Host no broker allowlist names.
fn metrics_router(store: Arc<Store>) -> Router {
    Router::new()
        .route("/metrics", get(metrics))
        .route("/healthz", get(healthz))
        .with_state(store)
}

/// One normalized `Host` authority: lowercase host without IPv6 brackets,
/// plus an optional port.
#[derive(Clone, Debug, PartialEq, Eq)]
struct HostAuthority {
    host: String,
    port: Option<u16>,
}

fn normalize_host(host: &str) -> String {
    host.trim_start_matches('[')
        .trim_end_matches(']')
        .to_ascii_lowercase()
}

/// Parse a request `Host` value. Userinfo and anything that is not an
/// RFC 3986 authority are refused. A bracketed host must be an IPv6 literal
/// (`[localhost]` is refused, not unwrapped), and a `:` after the host must
/// carry a valid port (`host:` and `host:99999` are refused, not read as
/// "no port").
fn parse_request_authority(raw: &str) -> Option<HostAuthority> {
    let authority = UriAuthority::try_from(raw.trim()).ok()?;
    let host = authority.host();
    if authority.as_str().contains('@') || host.is_empty() {
        return None;
    }
    if let Some(inner) = host.strip_prefix('[') {
        inner
            .strip_suffix(']')?
            .parse::<std::net::Ipv6Addr>()
            .ok()?;
    }
    let port = if authority.as_str().len() > host.len() {
        Some(authority.port_u16()?)
    } else {
        None
    };
    Some(HostAuthority {
        host: normalize_host(host),
        port,
    })
}

/// Parse an allowlist entry. Same rules as rmcp's `/mcp` check: an entry
/// that is not an authority (a bare IPv6 literal such as `::1`) is kept as a
/// host with no port.
fn parse_allowed_authority(raw: &str) -> Option<HostAuthority> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    parse_request_authority(raw).or_else(|| {
        Some(HostAuthority {
            host: normalize_host(raw),
            port: None,
        })
    })
}

/// A browser `Origin`: `null`, or a `(scheme, host, port)` tuple.
#[derive(Clone, Debug, PartialEq, Eq)]
enum BrowserOrigin {
    Null,
    Tuple {
        scheme: String,
        host: String,
        port: Option<u16>,
    },
}

fn parse_origin(raw: &str) -> Option<BrowserOrigin> {
    let raw = raw.trim();
    if raw.eq_ignore_ascii_case("null") {
        return Some(BrowserOrigin::Null);
    }
    let uri = Uri::try_from(raw).ok()?;
    let authority = uri.authority()?;
    if authority.as_str().contains('@') {
        return None;
    }
    Some(BrowserOrigin::Tuple {
        scheme: uri.scheme_str()?.to_ascii_lowercase(),
        host: normalize_host(authority.host()),
        port: authority.port_u16(),
    })
}

/// DNS-rebinding guard for every route on the main listener: `/mcp`,
/// every `/v1/*` route, the test-clock route and the 404 fallback.
///
/// - `Host` must be present, single, well formed and on the allowlist
///   (`SWB_MCP_ALLOWED_HOSTS`). An entry without a port admits any port; an
///   entry with a port admits only that port. Matching is case-insensitive.
///   With no `Host` header the URI authority (HTTP/2 `:authority`) is used;
///   an absolute-form request target's authority must be allowed as well.
/// - `Origin`, when present, must be on `SWB_ALLOWED_ORIGINS`, which is
///   empty by default, so every request a browser marks with `Origin`
///   (fetch, XHR, any form POST) is refused. Browsers omit `Origin` on
///   simple GETs such as image loads and navigations, so this does not stop
///   a blind cross-site GET with the real Host; see ADR-0001. The broker's
///   clients (`swb`, hooks, channel, MCP harnesses) send no Origin.
///
/// Every refusal is HTTP 403 with a JSON `error`, before any handler runs.
#[derive(Clone, Debug)]
pub struct RequestGuard {
    hosts: Vec<HostAuthority>,
    origins: Vec<BrowserOrigin>,
}

impl RequestGuard {
    /// Build a guard. An empty host allowlist is refused: rmcp reads an empty
    /// list as "allow every Host", which is exactly the rebinding hole.
    pub fn new(allowed_hosts: &[String], allowed_origins: &[String]) -> Result<Self, String> {
        let hosts: Vec<_> = allowed_hosts
            .iter()
            .filter_map(|h| parse_allowed_authority(h))
            .collect();
        if hosts.is_empty() {
            return Err("SWB_MCP_ALLOWED_HOSTS names no host; refusing to serve every Host".into());
        }
        let origins = allowed_origins
            .iter()
            .filter(|o| !o.trim().is_empty())
            .map(|o| {
                parse_origin(o).ok_or_else(|| format!("invalid SWB_ALLOWED_ORIGINS entry {o:?}"))
            })
            .collect::<Result<_, _>>()?;
        Ok(Self { hosts, origins })
    }

    fn allows_host(&self, host: &HostAuthority) -> bool {
        self.hosts.iter().any(|allowed| {
            allowed.host == host.host && allowed.port.is_none_or(|p| host.port == Some(p))
        })
    }

    fn check(&self, uri: &Uri, headers: &HeaderMap) -> Result<(), &'static str> {
        let mut values = headers.get_all(header::HOST).iter();
        let header_host = match (values.next(), values.next()) {
            (Some(_), Some(_)) => return Err("host not allowed"),
            (Some(value), None) => Some(
                value
                    .to_str()
                    .ok()
                    .and_then(parse_request_authority)
                    .ok_or("host not allowed")?,
            ),
            (None, _) => None,
        };
        // The URI authority is HTTP/2's `:authority`, or the authority of an
        // HTTP/1 absolute-form target, which RFC 9112 3.2.2 says takes
        // precedence over Host. Whichever is present must be allowed.
        let uri_host = match uri.authority() {
            Some(a) => Some(parse_request_authority(a.as_str()).ok_or("host not allowed")?),
            None => None,
        };
        if header_host.is_none() && uri_host.is_none() {
            return Err("missing host");
        }
        if !header_host
            .iter()
            .chain(uri_host.iter())
            .all(|host| self.allows_host(host))
        {
            return Err("host not allowed");
        }
        let mut origins = headers.get_all(header::ORIGIN).iter();
        match (origins.next(), origins.next()) {
            (None, _) => Ok(()),
            (Some(_), Some(_)) => Err("origin not allowed"),
            (Some(value), None) => {
                let origin = value
                    .to_str()
                    .ok()
                    .and_then(parse_origin)
                    .ok_or("origin not allowed")?;
                if self.origins.iter().any(|allowed| match (allowed, &origin) {
                    (BrowserOrigin::Null, BrowserOrigin::Null) => true,
                    (
                        BrowserOrigin::Tuple { scheme, host, port },
                        BrowserOrigin::Tuple {
                            scheme: s,
                            host: h,
                            port: p,
                        },
                    ) => scheme == s && host == h && (port.is_none() || port == p),
                    _ => false,
                }) {
                    Ok(())
                } else {
                    Err("origin not allowed")
                }
            }
        }
    }
}

async fn guard_request(
    State(guard): State<Arc<RequestGuard>>,
    request: Request,
    next: Next,
) -> Response {
    match guard.check(request.uri(), request.headers()) {
        Ok(()) => next.run(request).await,
        Err(reason) => (StatusCode::FORBIDDEN, Json(json!({"error":reason}))).into_response(),
    }
}

fn env_list(name: &str) -> Option<Vec<String>> {
    std::env::var(name).ok().map(|v| {
        v.split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .collect()
    })
}

#[cfg(test)]
fn router(store: Arc<Store>, allowed_hosts: Vec<String>) -> Router {
    let guard = RequestGuard::new(&allowed_hosts, &[]).expect("test allowlist");
    router_with_config(
        store,
        StreamableHttpServerConfig::default().with_allowed_hosts(allowed_hosts),
        guard,
        Router::new(),
    )
}

/// The main listener's app: `/mcp`, the `/v1/*` REST twin and `extra`, all
/// behind one [`RequestGuard`] layer.
fn router_with_config(
    store: Arc<Store>,
    config: StreamableHttpServerConfig,
    guard: RequestGuard,
    extra: Router,
) -> Router {
    let factory_store = Arc::clone(&store);
    let service = StreamableHttpService::new(
        move || {
            Ok(BrokerMcp {
                store: Arc::clone(&factory_store),
            })
        },
        LocalSessionManager::default().into(),
        config,
    );
    Router::new()
        .nest_service("/mcp", service)
        .route("/healthz", get(healthz))
        .route("/v1/register", post(register))
        .route("/v1/end", post(end))
        .route("/v1/peers", post(peers_post).get(peers_get))
        .route("/v1/send", post(send))
        .route("/v1/inbox", post(inbox_post).get(inbox_get))
        .route("/v1/ack", post(ack))
        .method_not_allowed_fallback(wrong_method)
        .fallback(not_found)
        .with_state(store)
        .merge(extra)
        .layer(middleware::from_fn_with_state(
            Arc::new(guard),
            guard_request,
        ))
}
pub async fn serve(store: Arc<Store>, listen: &str, metrics_listen: &str) -> io::Result<()> {
    serve_until(
        store,
        listen,
        metrics_listen,
        PRUNE_INTERVAL,
        Duration::from_secs(30),
        shutdown_signal(),
    )
    .await
}

async fn shutdown_signal() {
    #[cfg(unix)]
    {
        let mut terminate =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .expect("install SIGTERM handler");
        tokio::select! {
            _ = terminate.recv() => {},
            _ = tokio::signal::ctrl_c() => {},
        }
    }
    #[cfg(not(unix))]
    tokio::signal::ctrl_c()
        .await
        .expect("install Ctrl-C handler");
}

async fn serve_until<F>(
    store: Arc<Store>,
    listen: &str,
    metrics_listen: &str,
    prune_interval: Duration,
    drain_timeout: Duration,
    shutdown: F,
) -> io::Result<()>
where
    F: Future<Output = ()> + Send + 'static,
{
    serve_until_with(
        store,
        listen,
        metrics_listen,
        prune_interval,
        drain_timeout,
        shutdown,
        Router::new(),
    )
    .await
}

/// `serve_until` plus `extra` routes merged into the main listener. Production
/// passes an empty router; only the `test-clock` feature passes anything else.
async fn serve_until_with<F>(
    store: Arc<Store>,
    listen: &str,
    metrics_listen: &str,
    prune_interval: Duration,
    drain_timeout: Duration,
    shutdown: F,
    extra: Router,
) -> io::Result<()>
where
    F: Future<Output = ()> + Send + 'static,
{
    let main_listener = tokio::net::TcpListener::bind(listen).await?;
    let metrics_listener = tokio::net::TcpListener::bind(metrics_listen).await?;
    serve_bound_with(
        store,
        main_listener,
        metrics_listener,
        prune_interval,
        drain_timeout,
        shutdown,
        extra,
    )
    .await
}

#[cfg(test)]
async fn serve_bound<F>(
    store: Arc<Store>,
    main_listener: tokio::net::TcpListener,
    metrics_listener: tokio::net::TcpListener,
    prune_interval: Duration,
    drain_timeout: Duration,
    shutdown: F,
) -> io::Result<()>
where
    F: Future<Output = ()> + Send + 'static,
{
    serve_bound_with(
        store,
        main_listener,
        metrics_listener,
        prune_interval,
        drain_timeout,
        shutdown,
        Router::new(),
    )
    .await
}

async fn serve_bound_with<F>(
    store: Arc<Store>,
    main_listener: tokio::net::TcpListener,
    metrics_listener: tokio::net::TcpListener,
    prune_interval: Duration,
    drain_timeout: Duration,
    shutdown: F,
    extra: Router,
) -> io::Result<()>
where
    F: Future<Output = ()> + Send + 'static,
{
    let allowed_hosts = env_list("SWB_MCP_ALLOWED_HOSTS")
        .unwrap_or_else(|| vec!["localhost".into(), "127.0.0.1".into(), "::1".into()]);
    let allowed_origins = env_list("SWB_ALLOWED_ORIGINS").unwrap_or_default();
    let guard = RequestGuard::new(&allowed_hosts, &allowed_origins)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
    store.prune().map_err(io::Error::other)?;
    let config = StreamableHttpServerConfig::default().with_allowed_hosts(allowed_hosts);
    let mcp_cancel = config.cancellation_token.clone();
    let app = router_with_config(Arc::clone(&store), config, guard, extra);
    let metrics_app = metrics_router(Arc::clone(&store));
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    tokio::spawn(async move {
        shutdown.await;
        let _ = shutdown_tx.send(true);
    });
    let mut main_shutdown = shutdown_rx.clone();
    let mut metrics_shutdown = shutdown_rx.clone();
    let mut maintenance_shutdown = shutdown_rx;
    let mut drain_start = maintenance_shutdown.clone();
    let maintenance = async move {
        let mut interval =
            tokio::time::interval_at(tokio::time::Instant::now() + prune_interval, prune_interval);
        loop {
            tokio::select! {
                _ = maintenance_shutdown.changed() => break,
                _ = interval.tick() => {
                    let store = Arc::clone(&store);
                    match tokio::task::spawn_blocking(move || store.prune()).await {
                        Ok(Ok(())) => {},
                        Ok(Err(error)) => eprintln!("retention prune: {error}"),
                        Err(error) => eprintln!("retention prune task: {error}"),
                    }
                }
            }
        }
        Ok::<(), io::Error>(())
    };
    let listeners = async {
        tokio::try_join!(
            axum::serve(main_listener, app).with_graceful_shutdown(async move {
                let _ = main_shutdown.changed().await;
            }),
            axum::serve(metrics_listener, metrics_app).with_graceful_shutdown(async move {
                let _ = metrics_shutdown.changed().await;
            }),
            maintenance,
        )?;
        Ok::<(), io::Error>(())
    };
    tokio::pin!(listeners);
    tokio::select! {
        result = &mut listeners => result?,
        _ = drain_start.changed() => {
            match tokio::time::timeout(drain_timeout, &mut listeners).await {
                Ok(result) => result?,
                Err(_) => {
                    mcp_cancel.cancel();
                    tokio::time::timeout(Duration::from_secs(5), &mut listeners).await
                        .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "broker drain exceeded deadline"))??;
                }
            }
        }
    }
    Ok(())
}

/// Test-only time control for the spec live adapter (R-C262). Compiled only
/// with the `test-clock` Cargo feature, which no production target enables:
/// the `//crates/swb:swb` binary and the `//deploy:image` it ships are built
/// without it. A broker built with it still runs on the wall clock unless
/// `swb serve` is started with `SWB_TEST_CLOCK=1` on loopback listeners.
#[cfg(feature = "test-clock")]
pub mod test_clock {
    use super::*;
    use std::net::SocketAddr;
    use swb_store::ManualClock;

    /// Largest single advance: 400 days, past every retention bound.
    pub const MAX_ADVANCE_SECONDS: u64 = 400 * 86_400;

    /// `POST /v1/test/clock {"advance_seconds": n}` moves the clock forward
    /// by n seconds (0 reads it), runs one retention prune, and returns
    /// `{"now": <unix seconds>}`. The prune stands in for the production
    /// interval, which runs on the wall clock (R-C944: `send` and `inbox` no
    /// longer prune).
    pub fn router(clock: Arc<ManualClock>, store: Arc<Store>) -> Router {
        Router::new()
            .route("/v1/test/clock", post(advance))
            .with_state((clock, store))
    }

    async fn advance(
        State((clock, store)): State<(Arc<ManualClock>, Arc<Store>)>,
        Json(args): Json<Value>,
    ) -> Response {
        match args
            .get("advance_seconds")
            .and_then(Value::as_u64)
            .filter(|n| *n <= MAX_ADVANCE_SECONDS)
        {
            Some(n) => {
                let now = clock.advance(n as u32);
                match tokio::task::spawn_blocking(move || store.prune()).await {
                    Ok(Ok(())) => (StatusCode::OK, Json(json!({"now":now}))).into_response(),
                    _ => error_response(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "internal",
                        "retention prune failed",
                    ),
                }
            }
            None => error_response(
                StatusCode::BAD_REQUEST,
                "invalid_argument",
                "invalid advance_seconds",
            ),
        }
    }

    /// Refuse any listener that is not a loopback socket address.
    pub fn require_loopback(listen: &str) -> io::Result<()> {
        match listen.parse::<SocketAddr>() {
            Ok(address) if address.ip().is_loopback() => Ok(()),
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("test clock refuses non-loopback listener {listen:?}"),
            )),
        }
    }

    /// `serve` with the clock route on the main listener. Both listeners must
    /// be loopback addresses.
    pub async fn serve(
        store: Arc<Store>,
        clock: Arc<ManualClock>,
        listen: &str,
        metrics_listen: &str,
    ) -> io::Result<()> {
        require_loopback(listen)?;
        require_loopback(metrics_listen)?;
        let extra = router(clock, Arc::clone(&store));
        serve_until_with(
            store,
            listen,
            metrics_listen,
            PRUNE_INTERVAL,
            Duration::from_secs(30),
            shutdown_signal(),
            extra,
        )
        .await
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use axum::body::{Body, to_bytes};
        use axum::http::Request;
        use tower::ServiceExt;

        #[tokio::test]
        async fn clock_route_drives_store_time() {
            let clock = Arc::new(ManualClock::new(1_700_000_000));
            let store = Arc::new(Store::memory_with_clock(clock.clone()).unwrap());
            let app = router(clock, Arc::clone(&store)).merge(super::super::router(
                Arc::clone(&store),
                vec!["localhost".into()],
            ));
            let post = |body: &'static str| {
                Request::post("/v1/test/clock")
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .unwrap()
            };
            let response = app
                .clone()
                .oneshot(post(r#"{"advance_seconds":901}"#))
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            let body: Value =
                serde_json::from_slice(&to_bytes(response.into_body(), 4096).await.unwrap())
                    .unwrap();
            assert_eq!(body["now"], 1_700_000_901);
            assert_eq!(store.now(), 1_700_000_901);
            for bad in [
                r#"{"advance_seconds":-1}"#,
                r#"{"advance_seconds":"1"}"#,
                r#"{"advance_seconds":34560001}"#,
                r#"{}"#,
            ] {
                let response = app.clone().oneshot(post(bad)).await.unwrap();
                assert_eq!(response.status(), StatusCode::BAD_REQUEST);
            }
            assert_eq!(store.now(), 1_700_000_901);
        }

        #[tokio::test]
        async fn clock_advance_runs_the_retention_prune() {
            let clock = Arc::new(ManualClock::new(1_700_000_000));
            let store = Arc::new(Store::memory_with_clock(clock.clone()).unwrap());
            let to = "pi:sting:2:b";
            let id = store
                .send(&json!({"from":"claude:honey:1:a","to":to,"ticket":"none","body":"x"}))
                .unwrap()
                .envelope["msg_id"]
                .as_str()
                .unwrap()
                .to_owned();
            store.ack(to, &id).unwrap();
            let app = router(clock, Arc::clone(&store));
            let response = app
                .oneshot(
                    Request::post("/v1/test/clock")
                        .header("content-type", "application/json")
                        .body(Body::from(r#"{"advance_seconds":604800}"#))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            // The acked row is gone: a second ack is no longer "already acked".
            assert_eq!(store.ack(to, &id).unwrap_err(), "message unavailable");
        }

        #[tokio::test]
        async fn clock_route_is_behind_the_host_guard() {
            let clock = Arc::new(ManualClock::new(1_700_000_000));
            let store = Arc::new(Store::memory_with_clock(clock.clone()).unwrap());
            let app = super::super::router_with_config(
                Arc::clone(&store),
                StreamableHttpServerConfig::default(),
                RequestGuard::new(&["127.0.0.1".into()], &[]).unwrap(),
                router(clock, Arc::clone(&store)),
            );
            let post = |host: &'static str| {
                Request::post("/v1/test/clock")
                    .header("host", host)
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"advance_seconds":60}"#))
                    .unwrap()
            };
            let refused = app.clone().oneshot(post("attacker.example")).await.unwrap();
            assert_eq!(refused.status(), StatusCode::FORBIDDEN);
            assert_eq!(store.now(), 1_700_000_000);
            let allowed = app.oneshot(post("127.0.0.1:18080")).await.unwrap();
            assert_eq!(allowed.status(), StatusCode::OK);
            assert_eq!(store.now(), 1_700_000_060);
        }

        #[test]
        fn refuses_non_loopback_listeners() {
            assert!(require_loopback("127.0.0.1:18080").is_ok());
            assert!(require_loopback("[::1]:18080").is_ok());
            assert!(require_loopback("0.0.0.0:8080").is_err());
            assert!(require_loopback("100.85.46.118:8080").is_err());
            assert!(require_loopback("localhost:8080").is_err());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::{Body, to_bytes},
        http::Request,
    };
    use tower::ServiceExt;

    fn now_for_test() -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64
    }

    #[tokio::test]
    async fn idle_broker_prunes_retained_bodies_on_interval() {
        let path = std::env::temp_dir().join(format!(
            "swb-idle-prune-{}-{}.sqlite3",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let store = Arc::new(Store::open(&path).unwrap());
        let sent = store.send(&json!({"from":"claude:honey:1:a","to":"pi:sting:2:b","ticket":"none","body":"idle body"})).unwrap();
        let id = sent.envelope["msg_id"].as_str().unwrap();
        let main = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = main.local_addr().unwrap();
        let metrics = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
        let server = tokio::spawn(serve_bound(
            store,
            main,
            metrics,
            Duration::from_millis(20),
            Duration::from_secs(1),
            async move {
                let _ = stopped.await;
            },
        ));
        // A successful request proves startup pruning completed before the
        // row is aged. No send or inbox request occurs after that point.
        let ready = tokio::task::spawn_blocking(move || {
            use std::io::{Read, Write};
            let mut stream = std::net::TcpStream::connect(address).unwrap();
            stream
                .write_all(
                    b"GET /v1/peers HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
                )
                .unwrap();
            let mut response = String::new();
            stream.read_to_string(&mut response).unwrap();
            response
        })
        .await
        .unwrap();
        assert!(ready.contains("200 OK"));
        let db = rusqlite::Connection::open(&path).unwrap();
        assert_eq!(
            db.query_row("SELECT COUNT(*) FROM messages", [], |row| row
                .get::<_, i64>(0))
                .unwrap(),
            1
        );
        db.execute(
            "UPDATE messages SET created_at=unixepoch()-2592001 WHERE msg_id=?1",
            [id],
        )
        .unwrap();
        drop(db);
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                let count: i64 = rusqlite::Connection::open(&path)
                    .unwrap()
                    .query_row("SELECT COUNT(*) FROM messages", [], |row| row.get(0))
                    .unwrap();
                if count == 0 {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("idle prune did not run");
        stop.send(()).unwrap();
        server.await.unwrap().unwrap();
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(format!("{}-wal", path.display()));
        let _ = std::fs::remove_file(format!("{}-shm", path.display()));
    }

    #[tokio::test]
    async fn shutdown_waits_for_an_active_inbox_request() {
        let path = std::env::temp_dir().join(format!(
            "swb-drain-{}-{}.sqlite3",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let store = Arc::new(Store::open(&path).unwrap());
        store
            .register(
                &json!({"harness":"pi","host":"sting","pid":2,"session_id":"b","proc_start":"1"}),
            )
            .unwrap();
        rusqlite::Connection::open(&path)
            .unwrap()
            .execute("UPDATE sessions SET last_seen=unixepoch()-100", [])
            .unwrap();
        let main = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = main.local_addr().unwrap();
        let metrics = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
        let server = tokio::spawn(serve_bound(
            store,
            main,
            metrics,
            Duration::from_secs(3600),
            Duration::from_secs(2),
            async move {
                let _ = stopped.await;
            },
        ));
        let request = tokio::task::spawn_blocking(move || {
            use std::io::{Read, Write};
            let mut stream = std::net::TcpStream::connect(address).unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let body = r#"{"me":"pi:sting:2:b","wait_seconds":1}"#;
            write!(stream, "POST /v1/inbox HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
            let mut response = String::new();
            stream.read_to_string(&mut response).unwrap();
            response
        });
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                let seen: i64 = rusqlite::Connection::open(&path)
                    .unwrap()
                    .query_row(
                        "SELECT last_seen FROM sessions WHERE agent_id='pi:sting:2:b'",
                        [],
                        |row| row.get(0),
                    )
                    .unwrap();
                if seen >= now_for_test() - 2 {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("inbox handler did not start");
        stop.send(()).unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(!server.is_finished(), "shutdown dropped an active request");
        assert!(request.await.unwrap().contains("200 OK"));
        tokio::time::timeout(Duration::from_secs(2), server)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(format!("{}-wal", path.display()));
        let _ = std::fs::remove_file(format!("{}-shm", path.display()));
    }

    #[tokio::test]
    async fn shutdown_cancels_an_initialized_mcp_sse_stream_after_drain() {
        use std::io::{Read, Write};
        let main = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = main.local_addr().unwrap();
        let metrics = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
        let server = tokio::spawn(serve_bound(
            Arc::new(Store::memory().unwrap()),
            main,
            metrics,
            Duration::from_secs(3600),
            Duration::from_millis(150),
            async move {
                let _ = stopped.await;
            },
        ));
        let session = tokio::task::spawn_blocking(move || {
            let init = json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"drain-test","version":"1"}}}).to_string();
            let mut stream = std::net::TcpStream::connect(address).unwrap();
            stream.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
            write!(stream, "POST /mcp HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nAccept: application/json, text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", init.len(), init).unwrap();
            let mut response = String::new();
            stream.read_to_string(&mut response).unwrap();
            assert!(response.contains("200 OK"), "{response}");
            response.lines().find_map(|line| line.strip_prefix("mcp-session-id: ").map(str::trim).map(str::to_owned)).expect("MCP session header")
        }).await.unwrap();
        let (headers_tx, headers_rx) = tokio::sync::oneshot::channel();
        let sse = tokio::task::spawn_blocking(move || {
            let mut stream = std::net::TcpStream::connect(address).unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            write!(stream, "GET /mcp HTTP/1.1\r\nHost: localhost\r\nAccept: text/event-stream\r\nmcp-session-id: {session}\r\nConnection: close\r\n\r\n").unwrap();
            let mut headers = Vec::new();
            while !headers.ends_with(b"\r\n\r\n") {
                let mut byte = [0];
                stream.read_exact(&mut byte).unwrap();
                headers.push(byte[0]);
            }
            headers_tx
                .send(String::from_utf8(headers).unwrap())
                .unwrap();
            let mut rest = Vec::new();
            stream
                .read_to_end(&mut rest)
                .expect("SSE stream did not close");
            std::time::Instant::now()
        });
        let headers = tokio::time::timeout(Duration::from_secs(2), headers_rx)
            .await
            .unwrap()
            .unwrap();
        assert!(headers.contains("200 OK"), "{headers}");
        assert!(
            headers.to_ascii_lowercase().contains("text/event-stream"),
            "{headers}"
        );
        let stopped_at = std::time::Instant::now();
        stop.send(()).unwrap();
        let closed_at = tokio::time::timeout(Duration::from_secs(2), sse)
            .await
            .unwrap()
            .unwrap();
        assert!(closed_at.duration_since(stopped_at) >= Duration::from_millis(150));
        tokio::time::timeout(Duration::from_secs(2), server)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
    }
    async fn mcp_call(app: &Router, session: &str, id: u32, name: &str, args: Value) -> Value {
        let call = json!({"jsonrpc":"2.0","id":id,"method":"tools/call","params":{"name":name,"arguments":args}});
        let request = Request::builder()
            .method("POST")
            .uri("/mcp")
            .header("host", "localhost")
            .header("content-type", "application/json")
            .header("accept", "application/json, text/event-stream")
            .header("mcp-session-id", session)
            .body(Body::from(call.to_string()))
            .unwrap();
        let response = app.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), 65536).await.unwrap();
        let raw = std::str::from_utf8(&body).unwrap();
        let payload = raw
            .lines()
            .filter_map(|line| line.strip_prefix("data: "))
            .find(|line| line.trim_start().starts_with('{'))
            .unwrap_or(raw);
        let value: Value =
            serde_json::from_str(payload).unwrap_or_else(|error| panic!("{error}: {raw:?}"));
        assert!(value.get("error").is_none(), "{value}");
        value["result"]["structuredContent"].clone()
    }
    #[test]
    fn broker_stamps_peer() {
        assert_eq!(stamped_authority(), Authority::Peer);
    }
    #[test]
    fn audit_excludes_body_and_unapproved_state() {
        let store = Store::memory().unwrap();
        let args = json!({"from":"claude:honey:1:a","to":"pi:sting:2:b","ticket":"none","body":"secret sentinel"});
        let envelope = store.send(&args).unwrap().envelope;
        let record = audit_record("send", &args, &envelope, Some("keyed-hmac"));
        let line = record.to_string();
        assert!(!line.contains("secret sentinel"));
        assert_eq!(record["state"], Value::Null);
        assert_eq!(record["body_hmac"], "keyed-hmac");
        let mut invalid = args;
        invalid["state"] = json!("operator");
        assert_eq!(store.send(&invalid).unwrap_err(), "unknown envelope field");
    }
    #[test]
    fn lists_six_typed_and_described_tools() {
        assert_eq!(tools().len(), 6);
        assert_eq!(
            tools()
                .iter()
                .map(|t| t.name.to_string())
                .collect::<Vec<_>>(),
            TOOL_NAMES
        );
        for tool in tools() {
            let props = tool.input_schema["properties"].as_object().unwrap();
            for (key, schema) in props {
                assert!(
                    schema["description"]
                        .as_str()
                        .is_some_and(|d| !d.is_empty()),
                    "{}.{key} has no description",
                    tool.name
                );
            }
        }
        let peers = tools().into_iter().find(|t| t.name == "peers").unwrap();
        assert_eq!(peers.input_schema["properties"]["me"]["type"], "string");
        assert_eq!(peers.input_schema["properties"]["all"]["type"], "boolean");
        let end = tools().into_iter().find(|t| t.name == "end").unwrap();
        assert_eq!(end.input_schema["required"], json!(["me", "end_token"]));
        let send = tools().into_iter().find(|t| t.name == "send").unwrap();
        let props = &send.input_schema["properties"];
        for key in ["artifacts", "reply_expires", "reply_format"] {
            assert!(
                props.get(key).is_some(),
                "missing {key} in advertised send schema"
            );
        }
    }
    #[tokio::test]
    async fn rmcp_streamable_http_initialize_and_tools() {
        let app = router(Arc::new(Store::memory().unwrap()), vec!["localhost".into()]);
        let init = json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"swb-test","version":"1"}}});
        let request = Request::builder()
            .method("POST")
            .uri("/mcp")
            .header("host", "localhost")
            .header("content-type", "application/json")
            .header("accept", "application/json, text/event-stream")
            .body(Body::from(init.to_string()))
            .unwrap();
        let response = app.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let session = response
            .headers()
            .get("mcp-session-id")
            .expect("rmcp session id")
            .to_str()
            .unwrap()
            .to_owned();
        let body = to_bytes(response.into_body(), 65536).await.unwrap();
        assert!(String::from_utf8_lossy(&body).contains("agent-switchboard"));
        let list = json!({"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}});
        let request = Request::builder()
            .method("POST")
            .uri("/mcp")
            .header("host", "localhost")
            .header("content-type", "application/json")
            .header("accept", "application/json, text/event-stream")
            .header("mcp-session-id", &session)
            .body(Body::from(list.to_string()))
            .unwrap();
        let response = app.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), 65536).await.unwrap();
        assert!(String::from_utf8_lossy(&body).contains("register"));
        let from = "claude:honey:1:a";
        let to = "pi:sting:2:b";
        let first = mcp_call(
            &app,
            &session,
            3,
            "register",
            json!({"harness":"claude","host":"honey","pid":1,"session_id":"a","proc_start":"1"}),
        )
        .await;
        assert_eq!(first["agent_id"], from);
        let token = first["end_token"].as_str().unwrap().to_owned();
        let second = mcp_call(
            &app,
            &session,
            4,
            "register",
            json!({"harness":"pi","host":"sting","pid":2,"session_id":"b","proc_start":"2"}),
        )
        .await;
        assert_eq!(second["agent_id"], to);
        let peers = mcp_call(&app, &session, 5, "peers", json!({})).await;
        assert_eq!(peers["peers"].as_array().unwrap().len(), 2);
        let msg_id = "01ARZ3NDEKTSV4RRFFQ69G5FAV";
        let sent = mcp_call(&app, &session, 6, "send", json!({"from":from,"to":to,"ticket":"none","body":"hello","msg_id":msg_id,"authority":"operator"})).await;
        assert_eq!(sent["authority"], "peer");
        assert_eq!(sent["seq"], 1);
        let received = mcp_call(&app, &session, 7, "inbox", json!({"me":to})).await;
        assert_eq!(received["messages"][0]["msg_id"], msg_id);
        let acked = mcp_call(&app, &session, 8, "ack", json!({"me":to,"msg_id":msg_id})).await;
        assert_eq!(acked["state"], "acked");
        let empty = mcp_call(&app, &session, 9, "inbox", json!({"me":to})).await;
        assert!(empty["messages"].as_array().unwrap().is_empty());
        let ended = mcp_call(
            &app,
            &session,
            10,
            "end",
            json!({"me":from,"end_token":token}),
        )
        .await;
        assert_eq!(ended["state"], "ended");
        let live = mcp_call(&app, &session, 11, "peers", json!({})).await;
        assert_eq!(live["peers"].as_array().unwrap().len(), 1);
        let every = mcp_call(&app, &session, 12, "peers", json!({"all":true})).await;
        assert_eq!(every["peers"].as_array().unwrap().len(), 2);
        assert!(!every.to_string().contains("proc_start"));
    }
    /// Every state-bearing route on the main listener, with a request body
    /// that would mutate or read state if the guard let it through.
    const GUARDED_ROUTES: [(&str, &str, &str); 7] = [
        (
            "POST",
            "/v1/register",
            r#"{"harness":"claude","host":"honey","pid":1,"session_id":"a","proc_start":"1"}"#,
        ),
        (
            "POST",
            "/v1/end",
            r#"{"me":"claude:honey:1:a","end_token":"0000000000000000000000000000000000000000000000000000000000000000"}"#,
        ),
        ("POST", "/v1/peers", r#"{"me":"claude:honey:1:a"}"#),
        (
            "POST",
            "/v1/send",
            r#"{"from":"claude:honey:1:a","to":"pi:sting:2:b","ticket":"none","body":"rebound"}"#,
        ),
        ("POST", "/v1/inbox", r#"{"me":"pi:sting:2:b"}"#),
        (
            "POST",
            "/v1/ack",
            r#"{"me":"pi:sting:2:b","msg_id":"01ARZ3NDEKTSV4RRFFQ69G5FAV"}"#,
        ),
        (
            "POST",
            "/mcp",
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"rebind","version":"1"}}}"#,
        ),
    ];

    fn guarded_request(
        method: &str,
        uri: &str,
        body: &str,
        headers: &[(&str, &str)],
    ) -> Request<Body> {
        let mut builder = Request::builder()
            .method(method)
            .uri(uri)
            .header("content-type", "application/json")
            .header("accept", "application/json, text/event-stream");
        for (name, value) in headers {
            builder = builder.header(*name, *value);
        }
        builder.body(Body::from(body.to_owned())).unwrap()
    }

    async fn error_of(response: Response) -> String {
        let body = to_bytes(response.into_body(), 4096).await.unwrap();
        serde_json::from_slice::<Value>(&body).unwrap()["error"]
            .as_str()
            .unwrap()
            .to_owned()
    }

    async fn json_of(response: Response) -> Value {
        let body = to_bytes(response.into_body(), 65536).await.unwrap();
        serde_json::from_slice(&body).unwrap()
    }

    fn every() -> PeersQuery<'static> {
        PeersQuery {
            all: true,
            ..PeersQuery::default()
        }
    }

    fn seeded_store() -> Arc<Store> {
        let store = Arc::new(Store::memory().unwrap());
        store
            .register(
                &json!({"harness":"pi","host":"sting","pid":2,"session_id":"b","proc_start":"2"}),
            )
            .unwrap();
        store
    }

    fn state_snapshot(store: &Store) -> (usize, String) {
        (
            store.peers(&every()).unwrap()["peers"]
                .as_array()
                .unwrap()
                .len(),
            store.metrics().unwrap(),
        )
    }

    #[tokio::test]
    async fn rebinding_host_is_refused_on_every_route_before_any_handler() {
        let store = seeded_store();
        let before = state_snapshot(&store);
        let app = router(Arc::clone(&store), vec!["localhost".into()]);
        for host in [
            "attacker.example",
            "attacker.example:8080",
            "localhost.attacker.example",
            "user@localhost",
            "",
        ] {
            for (method, uri, body) in GUARDED_ROUTES {
                let response = app
                    .clone()
                    .oneshot(guarded_request(method, uri, body, &[("host", host)]))
                    .await
                    .unwrap();
                assert_eq!(
                    response.status(),
                    StatusCode::FORBIDDEN,
                    "{method} {uri} Host {host:?}"
                );
                assert_eq!(error_of(response).await, "host not allowed");
            }
        }
        assert_eq!(
            state_snapshot(&store),
            before,
            "a refused request changed state"
        );
    }

    #[tokio::test]
    async fn missing_or_duplicate_host_is_refused_on_every_route() {
        let store = seeded_store();
        let before = state_snapshot(&store);
        let app = router(Arc::clone(&store), vec!["localhost".into()]);
        for (method, uri, body) in GUARDED_ROUTES {
            let response = app
                .clone()
                .oneshot(guarded_request(method, uri, body, &[]))
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::FORBIDDEN, "{method} {uri}");
            assert_eq!(error_of(response).await, "missing host");
            let response = app
                .clone()
                .oneshot(guarded_request(
                    method,
                    uri,
                    body,
                    &[("host", "localhost"), ("host", "attacker.example")],
                ))
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::FORBIDDEN, "{method} {uri}");
        }
        assert_eq!(state_snapshot(&store), before);
    }

    #[tokio::test]
    async fn allowed_host_with_or_without_port_reaches_the_handler() {
        let app = router(
            Arc::new(Store::memory().unwrap()),
            vec![
                "localhost".into(),
                "::1".into(),
                "mcp-agents.example.ts.net".into(),
            ],
        );
        for host in [
            "localhost",
            "localhost:8080",
            "LOCALHOST:18080",
            "[::1]:8080",
            "mcp-agents.example.ts.net:8080",
        ] {
            let register = app
                .clone()
                .oneshot(guarded_request(
                    "POST",
                    "/v1/register",
                    GUARDED_ROUTES[0].2,
                    &[("host", host)],
                ))
                .await
                .unwrap();
            assert_eq!(register.status(), StatusCode::OK, "register Host {host:?}");
            let peers = app
                .clone()
                .oneshot(guarded_request("GET", "/v1/peers", "", &[("host", host)]))
                .await
                .unwrap();
            assert_eq!(peers.status(), StatusCode::OK, "peers Host {host:?}");
        }
    }

    #[tokio::test]
    async fn allowlist_entry_with_port_admits_only_that_port() {
        let store = Arc::new(Store::memory().unwrap());
        let guard = RequestGuard::new(&["swb.example:8080".into()], &[]).unwrap();
        let app = router_with_config(
            store,
            StreamableHttpServerConfig::default().with_allowed_hosts(["swb.example:8080"]),
            guard,
            Router::new(),
        );
        for (host, status) in [
            ("swb.example:8080", StatusCode::OK),
            ("swb.example:9999", StatusCode::FORBIDDEN),
            ("swb.example", StatusCode::FORBIDDEN),
        ] {
            let response = app
                .clone()
                .oneshot(guarded_request("GET", "/v1/peers", "", &[("host", host)]))
                .await
                .unwrap();
            assert_eq!(response.status(), status, "Host {host:?}");
        }
    }

    #[tokio::test]
    async fn browser_origin_is_refused_unless_allowlisted() {
        let store = seeded_store();
        let before = state_snapshot(&store);
        let app = router(Arc::clone(&store), vec!["localhost".into()]);
        for origin in [
            "http://attacker.example",
            "http://localhost:8080",
            "null",
            "not an origin",
        ] {
            for (method, uri, body) in GUARDED_ROUTES {
                let response = app
                    .clone()
                    .oneshot(guarded_request(
                        method,
                        uri,
                        body,
                        &[("host", "localhost:8080"), ("origin", origin)],
                    ))
                    .await
                    .unwrap();
                assert_eq!(
                    response.status(),
                    StatusCode::FORBIDDEN,
                    "{method} {uri} Origin {origin:?}"
                );
                assert_eq!(error_of(response).await, "origin not allowed");
            }
        }
        assert_eq!(state_snapshot(&store), before);
        let allowed = router_with_config(
            Arc::new(Store::memory().unwrap()),
            StreamableHttpServerConfig::default(),
            RequestGuard::new(&["localhost".into()], &["http://localhost:8080".into()]).unwrap(),
            Router::new(),
        );
        for (origin, status) in [
            ("http://localhost:8080", StatusCode::OK),
            ("http://localhost:9999", StatusCode::FORBIDDEN),
            ("https://localhost:8080", StatusCode::FORBIDDEN),
        ] {
            let response = allowed
                .clone()
                .oneshot(guarded_request(
                    "GET",
                    "/v1/peers",
                    "",
                    &[("host", "localhost:8080"), ("origin", origin)],
                ))
                .await
                .unwrap();
            assert_eq!(response.status(), status, "Origin {origin:?}");
        }
    }

    #[tokio::test]
    async fn unknown_paths_are_guarded_and_metrics_is_not() {
        let store = seeded_store();
        let app = router(Arc::clone(&store), vec!["localhost".into()]);
        for path in ["/nope", "/healthz"] {
            let refused = app
                .clone()
                .oneshot(guarded_request(
                    "GET",
                    path,
                    "",
                    &[("host", "attacker.example")],
                ))
                .await
                .unwrap();
            assert_eq!(refused.status(), StatusCode::FORBIDDEN, "{path}");
        }
        let not_found = app
            .clone()
            .oneshot(guarded_request(
                "GET",
                "/nope",
                "",
                &[("host", "localhost")],
            ))
            .await
            .unwrap();
        assert_eq!(not_found.status(), StatusCode::NOT_FOUND);
        assert_eq!(
            json_of(not_found).await,
            json!({"error":"no such route","code":"not_found"})
        );
        let health = app
            .oneshot(guarded_request(
                "GET",
                "/healthz",
                "",
                &[("host", "localhost")],
            ))
            .await
            .unwrap();
        assert_eq!(health.status(), StatusCode::OK);
        assert_eq!(
            json_of(health).await,
            json!({"status":"ok","version":VERSION})
        );
        let probe = metrics_router(Arc::clone(&store))
            .oneshot(guarded_request(
                "GET",
                "/healthz",
                "",
                &[("host", "10.42.0.17:9090")],
            ))
            .await
            .unwrap();
        assert_eq!(probe.status(), StatusCode::OK);
        // The metrics listener is deliberately unguarded (see metrics_router):
        // Prometheus scrapes it by pod address, and it serves only aggregate
        // counters.
        let metrics = metrics_router(store)
            .oneshot(guarded_request(
                "GET",
                "/metrics",
                "",
                &[("host", "10.42.0.17:9090")],
            ))
            .await
            .unwrap();
        assert_eq!(metrics.status(), StatusCode::OK);
        let body = to_bytes(metrics.into_body(), 4096).await.unwrap();
        let text = String::from_utf8(body.to_vec()).unwrap();
        assert!(text.contains("swb_messages_total"), "{text}");
        assert!(
            !text.contains("sting"),
            "metrics leaked an identity: {text}"
        );
    }

    #[tokio::test]
    async fn malformed_host_ports_and_brackets_are_refused() {
        let store = seeded_store();
        let before = state_snapshot(&store);
        let app = router(Arc::clone(&store), vec!["localhost".into(), "::1".into()]);
        for host in [
            "localhost:",
            "localhost:99999",
            "localhost:http",
            "[localhost]",
            "[localhost]:8080",
            "[::1",
        ] {
            for (method, uri, body) in GUARDED_ROUTES {
                let response = app
                    .clone()
                    .oneshot(guarded_request(method, uri, body, &[("host", host)]))
                    .await
                    .unwrap();
                assert_eq!(
                    response.status(),
                    StatusCode::FORBIDDEN,
                    "{method} {uri} Host {host:?}"
                );
                assert_eq!(error_of(response).await, "host not allowed");
            }
        }
        assert_eq!(state_snapshot(&store), before);
    }

    #[tokio::test]
    async fn absolute_form_target_authority_must_be_allowed_too() {
        let store = seeded_store();
        let before = state_snapshot(&store);
        let app = router(Arc::clone(&store), vec!["localhost".into()]);
        let refused = app
            .clone()
            .oneshot(guarded_request(
                "POST",
                "http://attacker.example/v1/register",
                GUARDED_ROUTES[0].2,
                &[("host", "localhost:8080")],
            ))
            .await
            .unwrap();
        assert_eq!(refused.status(), StatusCode::FORBIDDEN);
        assert_eq!(error_of(refused).await, "host not allowed");
        assert_eq!(state_snapshot(&store), before);
        let allowed = app
            .oneshot(guarded_request(
                "GET",
                "http://localhost:8080/v1/peers",
                "",
                &[("host", "localhost:8080")],
            ))
            .await
            .unwrap();
        assert_eq!(allowed.status(), StatusCode::OK);
    }

    #[test]
    fn empty_or_invalid_allowlists_are_refused() {
        assert!(RequestGuard::new(&[], &[]).is_err());
        assert!(RequestGuard::new(&[" ".into()], &[]).is_err());
        assert!(RequestGuard::new(&["localhost".into()], &["attacker.example".into()]).is_err());
        assert!(RequestGuard::new(&["localhost".into()], &["https://app.example".into()]).is_ok());
    }

    #[tokio::test]
    async fn conflict_is_opaque_over_rest() {
        let app = router(Arc::new(Store::memory().unwrap()), vec!["localhost".into()]);
        let msg = "01ARZ3NDEKTSV4RRFFQ69G5FAV";
        for body in ["A", "B"] {
            let args = json!({"from":"claude:honey:1:a","to":"pi:sting:2:b","body":body,"ticket":"none","msg_id":msg});
            let request = Request::builder()
                .method("POST")
                .uri("/v1/send")
                .header("host", "localhost")
                .header("content-type", "application/json")
                .body(Body::from(args.to_string()))
                .unwrap();
            let response = app.clone().oneshot(request).await.unwrap();
            if body == "B" {
                assert_eq!(response.status(), StatusCode::CONFLICT);
                assert_eq!(
                    json_of(response).await,
                    json!({"error":"msg_id conflict","code":"msg_id_conflict"})
                );
            } else {
                assert_eq!(response.status(), StatusCode::OK);
            }
        }
    }

    async fn call(app: &Router, method: &str, uri: &str, body: &str) -> Response {
        app.clone()
            .oneshot(guarded_request(method, uri, body, &[("host", "localhost")]))
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn state_changing_gets_answer_405_and_change_nothing() {
        let store = seeded_store();
        let app = router(Arc::clone(&store), vec!["localhost".into()]);
        store
            .send(
                &json!({"from":"claude:honey:1:a","to":"pi:sting:2:b","ticket":"none","body":"x"}),
            )
            .unwrap();
        let before = state_snapshot(&store);
        for uri in [
            "/v1/inbox?me=pi:sting:2:b",
            "/v1/inbox",
            "/v1/peers?me=pi:sting:2:b",
        ] {
            let response = call(&app, "GET", uri, "").await;
            assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED, "{uri}");
            assert_eq!(response.headers()[header::ALLOW], "POST", "{uri}");
            let body = json_of(response).await;
            assert_eq!(body["code"], "method_not_allowed", "{uri}");
            assert!(
                body["error"].as_str().unwrap().contains("POST /v1/"),
                "{uri}"
            );
        }
        assert_eq!(state_snapshot(&store), before);
        let inbox = store.inbox("pi:sting:2:b", 10).unwrap();
        assert_eq!(inbox["messages"][0]["delivery_count"], 1, "a GET fetched");
        let wrong = call(&app, "DELETE", "/v1/send", "").await;
        assert_eq!(wrong.status(), StatusCode::METHOD_NOT_ALLOWED);
        assert_eq!(json_of(wrong).await["code"], "method_not_allowed");
    }

    #[tokio::test]
    async fn post_peers_and_inbox_take_a_json_body() {
        let store = seeded_store();
        let app = router(Arc::clone(&store), vec!["localhost".into()]);
        let refreshed = call(
            &app,
            "POST",
            "/v1/peers",
            r#"{"me":"pi:sting:2:b","all":true}"#,
        )
        .await;
        assert_eq!(refreshed.status(), StatusCode::OK);
        let body = json_of(refreshed).await;
        assert_eq!(body["peers"][0]["agent_id"], "pi:sting:2:b");
        assert_eq!(body["next_cursor"], Value::Null);
        let passive = call(&app, "GET", "/v1/peers?all=1&host=sting&limit=5", "").await;
        assert_eq!(passive.status(), StatusCode::OK);
        assert_eq!(json_of(passive).await["peers"].as_array().unwrap().len(), 1);
        for (uri, code) in [
            ("/v1/peers?all=maybe", "invalid_argument"),
            ("/v1/peers?limit=-1", "invalid_argument"),
            ("/v1/peers?bogus=1", "invalid_argument"),
            ("/v1/peers?host=a%2Fb", "invalid_argument"),
        ] {
            let response = call(&app, "GET", uri, "").await;
            assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{uri}");
            assert_eq!(json_of(response).await["code"], code, "{uri}");
        }
        let inbox = call(
            &app,
            "POST",
            "/v1/inbox",
            r#"{"me":"pi:sting:2:b","limit":1}"#,
        )
        .await;
        assert_eq!(inbox.status(), StatusCode::OK);
        assert!(
            json_of(inbox).await["messages"]
                .as_array()
                .unwrap()
                .is_empty()
        );
    }

    #[tokio::test]
    async fn errors_are_typed_json() {
        let app = router(seeded_store(), vec!["localhost".into()]);
        let cases: [(&str, &str, StatusCode, &str); 7] = [
            (
                "/v1/peers",
                r#"{"me":"codex:honey:3:nobody"}"#,
                StatusCode::NOT_FOUND,
                "unregistered",
            ),
            (
                "/v1/ack",
                r#"{"me":"pi:sting:2:b","msg_id":"01ARZ3NDEKTSV4RRFFQ69G5FAV"}"#,
                StatusCode::NOT_FOUND,
                "message_unavailable",
            ),
            (
                "/v1/send",
                r#"{"from":"x","to":"y","ticket":"none","body":"b"}"#,
                StatusCode::BAD_REQUEST,
                "invalid_argument",
            ),
            (
                "/v1/peers",
                r#"{"all":"yes"}"#,
                StatusCode::BAD_REQUEST,
                "invalid_argument",
            ),
            (
                "/v1/send",
                "{not json",
                StatusCode::BAD_REQUEST,
                "invalid_json",
            ),
            ("/v1/send", "[]", StatusCode::BAD_REQUEST, "invalid_json"),
            (
                "/v1/inbox",
                r#"{"me":"pi:sting:2:b","wait_seconds":"5"}"#,
                StatusCode::BAD_REQUEST,
                "invalid_argument",
            ),
        ];
        for (uri, body, status, code) in cases {
            let response = call(&app, "POST", uri, body).await;
            assert_eq!(response.status(), status, "{uri} {body}");
            let value = json_of(response).await;
            assert_eq!(value["code"], code, "{uri} {body}: {value}");
            assert!(value["error"].as_str().is_some_and(|e| !e.is_empty()));
        }
        let untyped = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/send")
                    .header("host", "localhost")
                    .header("content-type", "text/plain")
                    .body(Body::from("{}"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(untyped.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
        assert_eq!(json_of(untyped).await["code"], "unsupported_media_type");
    }

    #[tokio::test]
    async fn end_over_rest_needs_the_registered_token() {
        let store = Arc::new(Store::memory().unwrap());
        let app = router(Arc::clone(&store), vec!["localhost".into()]);
        let registered =
            json_of(call(&app, "POST", "/v1/register", GUARDED_ROUTES[0].2).await).await;
        let token = registered["end_token"].as_str().unwrap();
        let legacy = call(
            &app,
            "POST",
            "/v1/end",
            r#"{"me":"claude:honey:1:a","proc_start":"1"}"#,
        )
        .await;
        assert_eq!(legacy.status(), StatusCode::BAD_REQUEST);
        assert!(error_of(legacy).await.contains("end_token"));
        let forged = call(&app, "POST", "/v1/end", GUARDED_ROUTES[1].2).await;
        assert_eq!(json_of(forged).await["state"], "unknown");
        let listed = json_of(call(&app, "GET", "/v1/peers", "").await).await;
        assert_eq!(listed["peers"][0]["state"], "live");
        assert!(listed["peers"][0].get("proc_start").is_none());
        let body = json!({"me":"claude:honey:1:a","end_token":token}).to_string();
        let ended = call(&app, "POST", "/v1/end", &body).await;
        assert_eq!(json_of(ended).await["state"], "ended");
        let listed = json_of(call(&app, "GET", "/v1/peers", "").await).await;
        assert!(listed["peers"].as_array().unwrap().is_empty());
    }

    #[test]
    fn audit_never_carries_the_end_token() {
        let store = Store::memory().unwrap();
        let args = json!({"harness":"claude","host":"honey","pid":1,"session_id":"a","proc_start":"secret-start"});
        let result = store.register(&args).unwrap();
        let token = result["end_token"].as_str().unwrap();
        let line = audit_record("register", &args, &result, None).to_string();
        assert!(!line.contains(token), "{line}");
        assert_eq!(
            audit_record("register", &args, &result, None)["agent_id"],
            "claude:honey:1:a"
        );
    }
}
