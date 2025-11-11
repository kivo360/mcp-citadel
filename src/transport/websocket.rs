//! WebSocket Transport for MCP Citadel
//! 
//! Provides bidirectional real-time communication as an alternative to SSE.

use anyhow::Result;
use axum::{
    extract::{
        ws::{Message, WebSocket},
        State, WebSocketUpgrade,
    },
    http::StatusCode,
    response::Response,
};
use futures::{SinkExt, StreamExt};
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{error, info, warn};

use super::http::AppState;
use crate::metrics;

/// Handle WebSocket upgrade at /ws endpoint
pub async fn handle_websocket(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> Result<Response, StatusCode> {
    info!("WebSocket connection requested");
    
    // Record WebSocket connection attempt
    metrics::record_websocket_connection("requested");
    
    Ok(ws.on_upgrade(move |socket| handle_socket(socket, state)))
}

/// Handle an established WebSocket connection
async fn handle_socket(socket: WebSocket, state: AppState) {
    info!("WebSocket connection established");
    metrics::record_websocket_connection("established");
    metrics::set_active_connections(1); // Simplified - would track properly in production
    
    let (mut sender, mut receiver) = socket.split();
    let session_id = uuid::Uuid::new_v4().to_string();
    
    info!("[ws_{}] New WebSocket session", &session_id[..8]);
    
    // Handle incoming messages
    while let Some(msg) = receiver.next().await {
        match msg {
            Ok(Message::Text(text)) => {
                info!("[ws_{}] Received message: {} bytes", &session_id[..8], text.len());
                
                // Parse JSON-RPC message
                match serde_json::from_str::<serde_json::Value>(&text) {
                    Ok(json_value) => {
                        let method = json_value
                            .get("method")
                            .and_then(|m| m.as_str())
                            .unwrap_or("unknown");
                        
                        // Extract server name (simplified)
                        let server_name = json_value
                            .get("params")
                            .and_then(|p| p.get("server"))
                            .and_then(|s| s.as_str())
                            .unwrap_or("unknown");
                        
                        info!("[ws_{}] Routing: method={} server={}", &session_id[..8], method, server_name);
                        
                        // Route to MCP server
                        let timer = metrics::MCPMessageTimer::new(server_name, method);
                        match state.manager.route_message(server_name, text.as_bytes()).await {
                            Ok(response) => {
                                timer.observe_duration("success");
                                
                                // Send response back via WebSocket
                                if let Ok(response_text) = String::from_utf8(response) {
                                    if let Err(e) = sender.send(Message::Text(response_text)).await {
                                        error!("[ws_{}] Failed to send response: {}", &session_id[..8], e);
                                        break;
                                    }
                                }
                            }
                            Err(e) => {
                                timer.observe_duration("error");
                                error!("[ws_{}] Routing error: {}", &session_id[..8], e);
                                
                                // Send error response
                                let error_response = serde_json::json!({
                                    "jsonrpc": "2.0",
                                    "id": json_value.get("id"),
                                    "error": {
                                        "code": -32603,
                                        "message": e.to_string()
                                    }
                                });
                                
                                if let Err(e) = sender.send(Message::Text(error_response.to_string())).await {
                                    error!("[ws_{}] Failed to send error: {}", &session_id[..8], e);
                                    break;
                                }
                            }
                        }
                    }
                    Err(e) => {
                        error!("[ws_{}] Invalid JSON: {}", &session_id[..8], e);
                        let error_response = serde_json::json!({
                            "jsonrpc": "2.0",
                            "error": {
                                "code": -32700,
                                "message": format!("Parse error: {}", e)
                            }
                        });
                        let _ = sender.send(Message::Text(error_response.to_string())).await;
                    }
                }
            }
            Ok(Message::Close(_)) => {
                info!("[ws_{}] Client closed connection", &session_id[..8]);
                break;
            }
            Ok(Message::Ping(data)) => {
                // Respond to ping with pong
                let _ = sender.send(Message::Pong(data)).await;
            }
            Ok(_) => {
                // Ignore other message types (binary, pong)
            }
            Err(e) => {
                error!("[ws_{}] WebSocket error: {}", &session_id[..8], e);
                break;
            }
        }
    }
    
    info!("[ws_{}] WebSocket connection closed", &session_id[..8]);
    metrics::record_websocket_connection("closed");
    metrics::set_active_connections(0);
}
