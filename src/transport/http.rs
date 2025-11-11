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
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use crate::config::HttpConfig;
use crate::router::HubManager;

/// MCP Protocol version supported
const MCP_PROTOCOL_VERSION: &str = "2025-06-18";

/// HTTP session state
#[derive(Debug, Clone)]
struct HttpSession {
    id: String,
    #[allow(dead_code)]
    created_at: Instant,
    last_activity: Instant,
    server_name: Option<String>,
    /// Channel for sending SSE events
    event_tx: Option<mpsc::Sender<Result<Event, Infallible>>>,
    /// Last event ID for resumability
    #[allow(dead_code)]
    last_event_id: u64,
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
        }
    }

    fn is_expired(&self, timeout: Duration) -> bool {
        self.last_activity.elapsed() > timeout
    }

    fn touch(&mut self) {
        self.last_activity = Instant::now();
    }

    #[allow(dead_code)]
    fn next_event_id(&mut self) -> String {
        self.last_event_id += 1;
        self.last_event_id.to_string()
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

/// Handle POST /mcp - Client sends JSON-RPC message
async fn handle_post(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<Response, StatusCode> {
    // 1. Validate Origin header (prevent DNS rebinding attacks)
    validate_origin(&headers)?;

    // 2. Check protocol version
    let protocol_version = headers
        .get("mcp-protocol-version")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("2025-03-26"); // Fallback for backwards compatibility

    if protocol_version != MCP_PROTOCOL_VERSION && protocol_version != "2025-03-26" {
        warn!("Unsupported protocol version: {}", protocol_version);
        return Err(StatusCode::BAD_REQUEST);
    }

    // 3. Get or create session
    let session_id = headers
        .get("mcp-session-id")
        .and_then(|v| v.to_str().ok())
        .map(String::from);

    let mut sessions = state.sessions.lock().await;
    
    // Parse JSON-RPC message to determine type
    let json_value: serde_json::Value = serde_json::from_slice(&body)
        .map_err(|_| StatusCode::BAD_REQUEST)?;

    let is_initialize = json_value
        .get("method")
        .and_then(|m| m.as_str())
        .map(|m| m == "initialize")
        .unwrap_or(false);

    let message_id = json_value.get("id");
    let is_request = message_id.is_some();

    // Handle session management
    let session = if is_initialize {
        // Initialize creates a new session
        let new_session = HttpSession::new();
        let session_id = new_session.id.clone();
        sessions.insert(session_id.clone(), new_session.clone());
        new_session
    } else if let Some(sid) = session_id {
        // Use existing session
        sessions.get_mut(&sid)
            .ok_or(StatusCode::NOT_FOUND)?
            .clone()
    } else {
        // No session ID and not initialize
        return Err(StatusCode::BAD_REQUEST);
    };

    // Extract server name from message
    let server_name = extract_server_name(&body);

    // 4. Route message to backend
    if let Some(name) = &server_name {
        match state.manager.route_message(name, &body).await {
            Ok(response) => {
                // Update session
                if let Some(session_mut) = sessions.get_mut(&session.id) {
                    session_mut.touch();
                    session_mut.server_name = server_name.clone();
                }
                drop(sessions);

                // For requests, we need to decide: SSE stream or direct response
                // For now, return direct JSON response (simple mode)
                if is_initialize {
                    // Return InitializeResult with session ID
                    Ok(Response::builder()
                        .status(StatusCode::OK)
                        .header(header::CONTENT_TYPE, "application/json")
                        .header("mcp-session-id", session.id)
                        .body(axum::body::Body::from(response))
                        .unwrap())
                } else if is_request {
                    // For other requests, return JSON response
                    Ok(Response::builder()
                        .status(StatusCode::OK)
                        .header(header::CONTENT_TYPE, "application/json")
                        .body(axum::body::Body::from(response))
                        .unwrap())
                } else {
                    // Notification - return 202 Accepted
                    Ok(Response::builder()
                        .status(StatusCode::ACCEPTED)
                        .body(axum::body::Body::empty())
                        .unwrap())
                }
            }
            Err(e) => {
                error!("Routing error: {}", e);
                
                // Return JSON-RPC error
                let error_response = serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": message_id,
                    "error": {
                        "code": -32603,
                        "message": e.to_string()
                    }
                });
                
                Ok(Response::builder()
                    .status(StatusCode::OK)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(axum::body::Body::from(error_response.to_string()))
                    .unwrap())
            }
        }
    } else {
        warn!("No server name found in message");
        Err(StatusCode::BAD_REQUEST)
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

    if let Some(last_id) = last_event_id {
        debug!("Client requesting resumption from event ID: {}", last_id);
        // TODO: Implement message replay for resumability
    }

    // Create SSE stream
    let (tx, rx) = mpsc::channel(100);
    
    // Store sender in session
    session.event_tx = Some(tx.clone());
    
    drop(sessions);

    // Create stream from receiver
    let stream = ReceiverStream::new(rx);

    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
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
