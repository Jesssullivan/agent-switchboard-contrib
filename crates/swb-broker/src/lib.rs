//! Tailnet broker REST and rmcp Streamable HTTP server (SWB-R02, SWB-R14,
//! SWB-R33, SWB-R46). Audit output omits bodies until SWB-R33/34 proof.

use axum::{
    Json, Router,
    extract::{Query, State},
    http::StatusCode,
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
use std::{collections::HashMap, io, sync::Arc};
use swb_proto::Authority;
use swb_store::Store;

pub const HOOK_TIMEOUT_MS: u64 = 2_000;
pub fn stamped_authority() -> Authority {
    Authority::Peer
}

fn operation(store: &Store, name: &str, args: &Value) -> Result<Value, String> {
    let mut created = true;
    let result = match name {
        "register" => store.register(args),
        "end" => store.end(
            args.get("me").and_then(Value::as_str).ok_or("missing me")?,
            args.get("proc_start")
                .and_then(Value::as_str)
                .ok_or("missing proc_start")?,
        ),
        "peers" => store.peers(
            args.get("me")
                .map(|v| v.as_str().ok_or("invalid me"))
                .transpose()?,
        ),
        "send" => {
            let outcome = store.send(args)?;
            created = outcome.created;
            Ok(outcome.envelope)
        }
        "inbox" => {
            let me = args.get("me").and_then(Value::as_str).ok_or("missing me")?;
            let limit = args
                .get("limit")
                .and_then(Value::as_u64)
                .unwrap_or(20)
                .min(100) as u32;
            let wait = args
                .get("wait_seconds")
                .and_then(Value::as_u64)
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

fn tools() -> Vec<Tool> {
    [
        ("register", "Register a self-asserted agent session", json!({"type":"object","required":["harness","host","pid","session_id","proc_start"],"properties":{"harness":{"type":"string"},"host":{"type":"string"},"pid":{"type":"integer"},"session_id":{"type":"string"},"proc_start":{"type":"string"}}})),
        ("peers", "List session leases; optional registered me renews only that caller's lease; omission is passive discovery", json!({"type":"object","properties":{"me":{"type":"string"}}})),
        ("send", "Send a peer-authority message", json!({"type":"object","required":["from","to","ticket","body"],"properties":{"from":{"type":"string"},"to":{"type":"string"},"ticket":{"type":"string"},"body":{"type":"string"},"msg_id":{"type":"string"},"thread_id":{"type":"string"},"in_reply_to":{"type":"string"},"operator_directed":{"type":"boolean"},"ruling":{"type":"string","minLength":1,"maxLength":512},"reply_expires":{"type":"string","format":"date-time"},"reply_format":{"type":"string","maxLength":512},"artifacts":{"type":"array","maxItems":20,"uniqueItems":true,"items":{"type":"string","minLength":1,"maxLength":2048}},"ttl_hours":{"type":"integer"}}})),
        ("inbox", "Fetch unacked messages", json!({"type":"object","required":["me"],"properties":{"me":{"type":"string"},"limit":{"type":"integer"},"wait_seconds":{"type":"integer"}}})),
        ("ack", "Acknowledge a received message", json!({"type":"object","required":["me","msg_id"],"properties":{"me":{"type":"string"},"msg_id":{"type":"string"}}})),
    ].into_iter().map(|(name,description,schema)| Tool::new(name,description,Arc::new(schema.as_object().expect("static object schema").clone()))).collect()
}

#[derive(Clone)]
struct BrokerMcp {
    store: Arc<Store>,
}
impl ServerHandler for BrokerMcp {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("agent-switchboard", "0.1.0"))
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
        if !matches!(
            name.as_str(),
            "register" | "peers" | "send" | "inbox" | "ack"
        ) {
            return Err(McpError::invalid_params("unknown tool", None));
        }
        let args = Value::Object(request.arguments.unwrap_or_default());
        let store = Arc::clone(&self.store);
        match tokio::task::spawn_blocking(move || operation(&store, &name, &args)).await {
            Ok(Ok(value)) => Ok(CallToolResult::structured(value).into()),
            Ok(Err(error)) => Ok(CallToolResult::error(vec![ContentBlock::text(error)]).into()),
            Err(error) => Err(McpError::internal_error(error.to_string(), None)),
        }
    }
}

async fn run_operation(store: Arc<Store>, name: &'static str, args: Value) -> Response {
    match tokio::task::spawn_blocking(move || operation(&store, name, &args)).await {
        Ok(Ok(value)) => (StatusCode::OK, Json(value)).into_response(),
        Ok(Err(error)) => {
            let status = if error == "msg_id conflict" {
                StatusCode::CONFLICT
            } else {
                StatusCode::BAD_REQUEST
            };
            (status, Json(json!({"error":error}))).into_response()
        }
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error":"broker unavailable"})),
        )
            .into_response(),
    }
}
async fn register(State(store): State<Arc<Store>>, Json(args): Json<Value>) -> Response {
    run_operation(store, "register", args).await
}
async fn end(State(store): State<Arc<Store>>, Json(args): Json<Value>) -> Response {
    run_operation(store, "end", args).await
}
async fn send(State(store): State<Arc<Store>>, Json(args): Json<Value>) -> Response {
    run_operation(store, "send", args).await
}
async fn ack(State(store): State<Arc<Store>>, Json(args): Json<Value>) -> Response {
    run_operation(store, "ack", args).await
}
async fn peers(
    State(store): State<Arc<Store>>,
    Query(query): Query<HashMap<String, String>>,
) -> Response {
    let args = query
        .get("me")
        .map_or_else(|| json!({}), |me| json!({"me":me}));
    run_operation(store, "peers", args).await
}
async fn inbox(
    State(store): State<Arc<Store>>,
    Query(query): Query<HashMap<String, String>>,
) -> Response {
    let args = json!({"me":query.get("me"),"limit":query.get("limit").and_then(|v|v.parse::<u64>().ok()),"wait_seconds":query.get("wait_seconds").and_then(|v|v.parse::<u64>().ok())});
    run_operation(store, "inbox", args).await
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
fn router(store: Arc<Store>, allowed_hosts: Vec<String>) -> Router {
    let factory_store = Arc::clone(&store);
    let config = StreamableHttpServerConfig::default().with_allowed_hosts(allowed_hosts);
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
        .route("/v1/register", post(register))
        .route("/v1/end", post(end))
        .route("/v1/peers", get(peers))
        .route("/v1/send", post(send))
        .route("/v1/inbox", get(inbox))
        .route("/v1/ack", post(ack))
        .with_state(store)
}
pub async fn serve(store: Arc<Store>, listen: &str, metrics_listen: &str) -> io::Result<()> {
    let allowed_hosts = std::env::var("SWB_MCP_ALLOWED_HOSTS")
        .ok()
        .map(|v| {
            v.split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_else(|| vec!["localhost".into(), "127.0.0.1".into(), "::1".into()]);
    let main_listener = tokio::net::TcpListener::bind(listen).await?;
    let metrics_listener = tokio::net::TcpListener::bind(metrics_listen).await?;
    let app = router(Arc::clone(&store), allowed_hosts);
    let metrics_app = Router::new()
        .route("/metrics", get(metrics))
        .with_state(store);
    tokio::try_join!(
        axum::serve(main_listener, app),
        axum::serve(metrics_listener, metrics_app)
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::{Body, to_bytes},
        http::Request,
    };
    use tower::ServiceExt;
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
    fn lists_five_typed_tools() {
        assert_eq!(tools().len(), 5);
        assert!(
            tools()
                .iter()
                .all(|t| t.input_schema.contains_key("properties"))
        );
        let peers = tools().into_iter().find(|t| t.name == "peers").unwrap();
        assert_eq!(peers.input_schema["properties"]["me"]["type"], "string");
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
                .header("content-type", "application/json")
                .body(Body::from(args.to_string()))
                .unwrap();
            let response = app.clone().oneshot(request).await.unwrap();
            if body == "B" {
                assert_eq!(response.status(), StatusCode::CONFLICT);
                let content = to_bytes(response.into_body(), 65536).await.unwrap();
                assert_eq!(content.as_ref(), br#"{"error":"msg_id conflict"}"#);
            } else {
                assert_eq!(response.status(), StatusCode::OK);
            }
        }
    }
}
