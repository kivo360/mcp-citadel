//! HTTP/SSE Transport for MCP Citadel
//! Implements the Streamable HTTP transport from MCP specification 2025-06-18

use anyhow::Result;
use axum::{
    extract::State,
    http::{header, HeaderMap, StatusCode},
    response::{
        sse::{Event, KeepAlive, Sse},
        Response,
    },
    routing::post,
    Router,
};
use headers::{HeaderMapExt, Origin};
use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, Mutex};
use tokio_stream::{wrappers::ReceiverStream, Stream};
use futures::StreamExt;
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::config::HttpConfig;
use crate::router::HubManager;

/// MCP Protocol version supported
const MCP_PROTOCOL_VERSION: &str = "2025-06-18";

/// Buffered message for replay
#[derive(Debug, Clone)]
struct BufferedMessage {
    event_id: u64,
    event_type: Option<String>,
    data: String,
}

/// HTTP session state
#[derive(Debug, Clone)]
struct HttpSession {
    id: String,
    #[allow(dead_code)]
    created_at: Instant,
    last_activity: Instant,
    server_name: Option<String>,
    /// Channel for sending SSE events (bidirectional communication)
    event_tx: Option<mpsc::Sender<Result<Event, Infallible>>>,
    /// Last event ID for resumability
    last_event_id: u64,
    /// Buffer of recent messages for replay (max 100 messages)
    message_buffer: Vec<BufferedMessage>,
}

impl HttpSession {
    fn new() -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            created_at: Instant::now(),
            last_activity: Instant::now(),
            server_name: None,
            event_tx: None,
            last_event_id: 0,
            message_buffer: Vec::new(),
        }
    }

    fn is_expired(&self, timeout: Duration) -> bool {
        self.last_activity.elapsed() > timeout
    }

    fn touch(&mut self) {
        self.last_activity = Instant::now();
    }

    fn next_event_id(&mut self) -> u64 {
        self.last_event_id += 1;
        self.last_event_id
    }

    fn buffer_message(&mut self, event_id: u64, event_type: Option<String>, data: String) {
        const MAX_BUFFER_SIZE: usize = 100;
        
        self.message_buffer.push(BufferedMessage {
            event_id,
            event_type,
            data,
        });
        
        // Keep buffer size limited
        if self.message_buffer.len() > MAX_BUFFER_SIZE {
            self.message_buffer.remove(0);
        }
    }

    fn get_messages_after(&self, last_event_id: u64) -> Vec<BufferedMessage> {
        self.message_buffer
            .iter()
            .filter(|msg| msg.event_id > last_event_id)
            .cloned()
            .collect()
    }
}

/// Shared application state
#[derive(Clone)]
struct AppState {
    manager: Arc<HubManager>,
    sessions: Arc<Mutex<HashMap<String, HttpSession>>>,
    config: HttpConfig,
}

/// HTTP transport server
pub struct HttpTransport {
    config: HttpConfig,
    manager: Arc<HubManager>,
}

impl HttpTransport {
    pub fn new(config: HttpConfig, manager: Arc<HubManager>) -> Self {
        Self { config, manager }
    }

    /// Start the HTTP server
    pub async fn start(self) -> Result<()> {
        let addr = format!("{}:{}", self.config.host, self.config.port);
        
        let state = AppState {
            manager: self.manager,
            sessions: Arc::new(Mutex::new(HashMap::new())),
            config: self.config.clone(),
        };

        // Start session cleanup task
        let cleanup_state = state.clone();
        tokio::spawn(async move {
            session_cleanup_task(cleanup_state).await;
        });

        let app = Router::new()
            .route("/mcp", post(handle_post))
            .route("/mcp", axum::routing::get(handle_get))
            .with_state(state);

        info!("🌐 HTTP transport listening on http://{}", addr);
        
        let listener = tokio::net::TcpListener::bind(&addr).await?;
        axum::serve(listener, app).await?;

        Ok(())
    }
}

/// Response type for handle_post - either JSON or SSE
enum PostResponse {
    Json(Response<axum::body::Body>),
    Sse(Sse<std::pin::Pin<Box<dyn Stream<Item = Result<Event, Infallible>> + Send>>>),
}

impl axum::response::IntoResponse for PostResponse {
    fn into_response(self) -> Response<axum::body::Body> {
        match self {
            PostResponse::Json(r) => r,
            PostResponse::Sse(sse) => sse.into_response(),
        }
    }
}

/// Handle POST /mcp - Client sends JSON-RPC message (smart response: JSON or SSE)
async fn handle_post(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<PostResponse, StatusCode> {
    // 1. Validate Origin header
    validate_origin(&headers)?;

    // 2. Check protocol version
    let protocol_version = headers
        .get("mcp-protocol-version")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("2025-03-26");

    if protocol_version != MCP_PROTOCOL_VERSION && protocol_version != "2025-03-26" {
        warn!("Unsupported protocol version: {}", protocol_version);
        return Err(StatusCode::BAD_REQUEST);
    }

    // 3. Parse message
    let json_value: serde_json::Value = serde_json::from_slice(&body)
        .map_err(|_| StatusCode::BAD_REQUEST)?;

    let method = json_value
        .get("method")
        .and_then(|m| m.as_str())
        .unwrap_or("");
    
    let is_initialize = method == "initialize";
    let use_streaming = needs_streaming(method);

    // 4. Get or create session
    let session_id = headers
        .get("mcp-session-id")
        .and_then(|v| v.to_str().ok())
        .map(String::from);

    let mut sessions = state.sessions.lock().await;
    
    let session = if is_initialize {
        let new_session = HttpSession::new();
        let sid = new_session.id.clone();
        sessions.insert(sid.clone(), new_session.clone());
        new_session
    } else if let Some(sid) = session_id {
        sessions.get_mut(&sid)
            .ok_or(StatusCode::NOT_FOUND)?
            .clone()
    } else {
        return Err(StatusCode::BAD_REQUEST);
    };

    let session_id = session.id.clone();
    
    // Extract server name
    let server_name = extract_server_name(&body)
        .ok_or(StatusCode::BAD_REQUEST)?;

    // 5. Smart response mode: JSON for simple ops, SSE for streaming
    if !use_streaming {
        // Direct JSON response for simple operations
        drop(sessions);
        
        let manager = state.manager.clone();
        match manager.route_message(&server_name, &body).await {
            Ok(response) => {
                Ok(PostResponse::Json(
                    Response::builder()
                        .status(StatusCode::OK)
                        .header(header::CONTENT_TYPE, "application/json")
                        .body(axum::body::Body::from(response))
                        .unwrap()
                ))
            }
            Err(e) => {
                error!("Routing error for {}: {}", method, e);
                
                // Return JSON error response
                let error_json = serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": json_value.get("id"),
                    "error": {
                        "code": -32603,
                        "message": e.to_string(),
                        "data": {
                            "type": "routing_error",
                            "server": server_name
                        }
                    }
                });
                
                Ok(PostResponse::Json(
                    Response::builder()
                        .status(StatusCode::OK)
                        .header(header::CONTENT_TYPE, "application/json")
                        .body(axum::body::Body::from(error_json.to_string()))
                        .unwrap()
                ))
            }
        }
    } else {
        // SSE streaming for long-running/bidirectional operations
        let (tx, rx) = mpsc::channel(100);
        
        // Get next event ID for this session
        let event_id = if let Some(session_mut) = sessions.get_mut(&session_id) {
            session_mut.touch();
            session_mut.server_name = Some(server_name.clone());
            session_mut.event_tx = Some(tx.clone());
            session_mut.next_event_id()
        } else {
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        };
        
        let sessions_arc = state.sessions.clone();
        drop(sessions);

        // 6. Spawn async task to handle backend communication
        let manager = state.manager.clone();
        let body_clone = body.to_vec();
        let session_id_clone = session_id.clone();
        let json_id = json_value.get("id").cloned();
        
        tokio::spawn(async move {
            // Route message to backend (non-blocking for this HTTP handler)
            match manager.route_message(&server_name, &body_clone).await {
                Ok(response) => {
                    // Parse response to extract event data
                    if let Ok(json) = std::str::from_utf8(&response) {
                        let event = Event::default()
                            .id(event_id.to_string())
                            .data(json.trim_end());
                        
                        // Buffer the message for replay
                        let mut sessions = sessions_arc.lock().await;
                        if let Some(session) = sessions.get_mut(&session_id_clone) {
                            session.buffer_message(event_id, None, json.trim_end().to_string());
                        }
                        drop(sessions);
                        
                        // Send via SSE
                        let _ = tx.send(Ok(event)).await;
                    } else {
                        error!("Failed to parse response as UTF-8");
                        
                        // Send parse error
                        let error_event = Event::default()
                            .event("error")
                            .data(serde_json::json!({
                                "code": -32700,
                                "message": "Parse error: Invalid UTF-8 response",
                                "data": { "type": "parse_error" }
                            }).to_string());
                        let _ = tx.send(Ok(error_event)).await;
                    }
                }
                Err(e) => {
                    error!("Routing error: {}", e);
                    
                    // Enhanced error with type categorization
                    let (error_code, error_type) = if e.to_string().contains("not found") {
                        (-32001, "server_not_found")
                    } else if e.to_string().contains("timeout") {
                        (-32002, "timeout")
                    } else if e.to_string().contains("crashed") {
                        (-32003, "server_crash")
                    } else {
                        (-32603, "internal_error")
                    };
                    
                    let error_json = serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": json_id,
                        "error": {
                            "code": error_code,
                            "message": e.to_string(),
                            "data": {
                                "type": error_type,
                                "server": server_name
                            }
                        }
                    });
                    
                    let error_event = Event::default()
                        .event("error")
                        .data(error_json.to_string());
                    let _ = tx.send(Ok(error_event)).await;
                }
            }
        });

        // 7. Return SSE stream immediately
        let base_stream = ReceiverStream::new(rx);
        
        // For initialize, prepend session event
        let stream: std::pin::Pin<Box<dyn Stream<Item = Result<Event, Infallible>> + Send>> = if is_initialize {
            // Include session ID in first event
            let init_event = Event::default()
                .event("session")
                .data(format!("{{\"sessionId\":\"{}\"}}", session_id));
            
            // Prepend session event to stream
            let session_stream = futures::stream::once(async move { Ok(init_event) });
            Box::pin(futures::StreamExt::chain(session_stream, base_stream))
        } else {
            Box::pin(base_stream)
        };
        
        Ok(PostResponse::Sse(Sse::new(stream).keep_alive(KeepAlive::default())))
    }
}

/// Handle GET /mcp - Client opens SSE stream
async fn handle_get(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, StatusCode> {
    // Validate Origin
    validate_origin(&headers)?;

    // Get session ID
    let session_id = headers
        .get("mcp-session-id")
        .and_then(|v| v.to_str().ok())
        .ok_or(StatusCode::BAD_REQUEST)?;

    let mut sessions = state.sessions.lock().await;
    let session = sessions
        .get_mut(session_id)
        .ok_or(StatusCode::NOT_FOUND)?;

    session.touch();

    // Check for resumption via Last-Event-ID
    let last_event_id = headers
        .get("last-event-id")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok());

    // Get buffered messages for replay
    let replay_messages = if let Some(last_id) = last_event_id {
        let msgs = session.get_messages_after(last_id);
        info!("Client resuming from event {}: replaying {} messages", last_id, msgs.len());
        msgs
    } else {
        Vec::new()
    };

    // Create SSE stream
    let (tx, rx) = mpsc::channel(100);
    
    // Store sender in session
    session.event_tx = Some(tx.clone());
    
    drop(sessions);

    // Replay buffered messages if resuming
    if !replay_messages.is_empty() {
        tokio::spawn(async move {
            for msg in replay_messages {
                let mut event = Event::default()
                    .id(msg.event_id.to_string())
                    .data(msg.data);
                
                if let Some(event_type) = msg.event_type {
                    event = event.event(event_type);
                }
                
                if tx.send(Ok(event)).await.is_err() {
                    break; // Client disconnected
                }
            }
        });
    }

    // Create stream from receiver
    let stream = ReceiverStream::new(rx);

    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

/// Determine if a method requires SSE streaming
fn needs_streaming(method: &str) -> bool {
    // Methods that need streaming:
    // - initialize (handshake, needs session event)
    // - sampling/createMessage (LLM responses, can be long)
    // - Long-running operations
    // - Server-initiated requests/notifications
    
    matches!(method,
        "initialize" 
        | "initialized"
        | "sampling/createMessage"
        | "roots/list_changed"
        | "notifications/cancelled"
        | "notifications/progress"
    )
}

/// Validate Origin header to prevent DNS rebinding attacks
fn validate_origin(headers: &HeaderMap) -> Result<(), StatusCode> {
    // In production, you should validate against allowed origins
    // For now, we require localhost origins only
    
    if let Some(origin) = headers.typed_get::<Origin>() {
        let origin_str = origin.to_string();
        
        // Allow localhost, 127.0.0.1, and null origin (for testing)
        if origin_str.contains("localhost") 
            || origin_str.contains("127.0.0.1")
            || origin_str == "null" {
            Ok(())
        } else {
            warn!("Rejected non-localhost origin: {}", origin_str);
            Err(StatusCode::FORBIDDEN)
        }
    } else {
        // No origin header - allow for now (some clients don't send it)
        // In production, you might want to require this
        Ok(())
    }
}

/// Extract server name from JSON-RPC message
fn extract_server_name(message: &[u8]) -> Option<String> {
    let text = std::str::from_utf8(message).ok()?;
    let value: serde_json::Value = serde_json::from_str(text).ok()?;

    // Try params.server
    if let Some(params) = value.get("params") {
        if let Some(server) = params.get("server") {
            return server.as_str().map(String::from);
        }
    }

    // Try method prefix (e.g., "github/tools/list")
    if let Some(method) = value.get("method") {
        if let Some(method_str) = method.as_str() {
            if let Some(server) = method_str.split('/').next() {
                return Some(server.to_string());
            }
        }
    }

    None
}

/// Background task to cleanup expired sessions
async fn session_cleanup_task(state: AppState) {
    let mut interval = tokio::time::interval(Duration::from_secs(60));
    
    loop {
        interval.tick().await;
        
        let timeout = Duration::from_secs(state.config.session_timeout_secs);
        let mut sessions = state.sessions.lock().await;
        
        let expired: Vec<String> = sessions
            .iter()
            .filter(|(_, session)| session.is_expired(timeout))
            .map(|(id, _)| id.clone())
            .collect();
        
        for id in expired {
            info!("Cleaning up expired session: {}", id);
            sessions.remove(&id);
        }
    }
}
