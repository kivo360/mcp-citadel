//! Prometheus Metrics for MCP Citadel
//!
//! Tracks request count, latency, active sessions, errors, and MCP server health.

use lazy_static::lazy_static;
use prometheus::{
    register_counter_vec, register_gauge, register_histogram_vec, CounterVec, Encoder, Gauge,
    HistogramVec, TextEncoder,
};
use std::time::Instant;

lazy_static! {
    // HTTP request metrics
    pub static ref HTTP_REQUESTS_TOTAL: CounterVec = register_counter_vec!(
        "mcp_citadel_http_requests_total",
        "Total number of HTTP requests",
        &["method", "endpoint", "status"]
    )
    .unwrap();

    pub static ref HTTP_REQUEST_DURATION_SECONDS: HistogramVec = register_histogram_vec!(
        "mcp_citadel_http_request_duration_seconds",
        "HTTP request latency in seconds",
        &["method", "endpoint"],
        vec![0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0]
    )
    .unwrap();

    // Session metrics
    pub static ref ACTIVE_SESSIONS: Gauge = register_gauge!(
        "mcp_citadel_active_sessions",
        "Number of active HTTP sessions"
    )
    .unwrap();

    pub static ref TOTAL_SESSIONS_CREATED: CounterVec = register_counter_vec!(
        "mcp_citadel_sessions_created_total",
        "Total number of sessions created",
        &["transport"]
    )
    .unwrap();

    pub static ref SESSION_DURATION_SECONDS: HistogramVec = register_histogram_vec!(
        "mcp_citadel_session_duration_seconds",
        "Session lifetime in seconds",
        &["transport"],
        vec![1.0, 10.0, 60.0, 300.0, 600.0, 1800.0, 3600.0]
    )
    .unwrap();

    // MCP server metrics
    pub static ref MCP_MESSAGES_TOTAL: CounterVec = register_counter_vec!(
        "mcp_citadel_mcp_messages_total",
        "Total number of MCP messages routed",
        &["server", "method", "status"]
    )
    .unwrap();

    pub static ref MCP_MESSAGE_DURATION_SECONDS: HistogramVec = register_histogram_vec!(
        "mcp_citadel_mcp_message_duration_seconds",
        "MCP message processing latency in seconds",
        &["server", "method"],
        vec![0.01, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0, 30.0, 60.0]
    )
    .unwrap();

    pub static ref MCP_SERVER_UP: Gauge = register_gauge!(
        "mcp_citadel_mcp_server_up",
        "MCP servers currently up (1) or down (0)"
    )
    .unwrap();

    // Error metrics
    pub static ref ERRORS_TOTAL: CounterVec = register_counter_vec!(
        "mcp_citadel_errors_total",
        "Total number of errors",
        &["type", "server"]
    )
    .unwrap();

    // Buffer metrics
    pub static ref MESSAGE_BUFFER_SIZE: Gauge = register_gauge!(
        "mcp_citadel_message_buffer_size",
        "Total messages in all session buffers"
    )
    .unwrap();

    pub static ref MESSAGE_REPLAY_TOTAL: CounterVec = register_counter_vec!(
        "mcp_citadel_message_replay_total",
        "Total number of message replays",
        &["session_id"]
    )
    .unwrap();

    // Connection metrics
    pub static ref ACTIVE_CONNECTIONS: Gauge = register_gauge!(
        "mcp_citadel_active_connections",
        "Number of active connections (HTTP + WebSocket)"
    )
    .unwrap();

    pub static ref WEBSOCKET_CONNECTIONS_TOTAL: CounterVec = register_counter_vec!(
        "mcp_citadel_websocket_connections_total",
        "Total WebSocket connections",
        &["status"]
    )
    .unwrap();
}

/// Request timer for tracking latency
pub struct RequestTimer {
    start: Instant,
    method: String,
    endpoint: String,
}

impl RequestTimer {
    pub fn new(method: impl Into<String>, endpoint: impl Into<String>) -> Self {
        Self {
            start: Instant::now(),
            method: method.into(),
            endpoint: endpoint.into(),
        }
    }

    pub fn observe_duration(self) {
        let duration = self.start.elapsed().as_secs_f64();
        HTTP_REQUEST_DURATION_SECONDS
            .with_label_values(&[&self.method, &self.endpoint])
            .observe(duration);
    }
}

/// MCP message timer for tracking backend latency
pub struct MCPMessageTimer {
    start: Instant,
    server: String,
    method: String,
}

impl MCPMessageTimer {
    pub fn new(server: impl Into<String>, method: impl Into<String>) -> Self {
        Self {
            start: Instant::now(),
            server: server.into(),
            method: method.into(),
        }
    }

    pub fn observe_duration(self, status: &str) {
        let duration = self.start.elapsed().as_secs_f64();
        MCP_MESSAGE_DURATION_SECONDS
            .with_label_values(&[&self.server, &self.method])
            .observe(duration);
        
        MCP_MESSAGES_TOTAL
            .with_label_values(&[&self.server, &self.method, status])
            .inc();
    }
}

/// Export metrics in Prometheus text format
pub fn export_metrics() -> Result<String, Box<dyn std::error::Error>> {
    let encoder = TextEncoder::new();
    let metric_families = prometheus::gather();
    let mut buffer = vec![];
    encoder.encode(&metric_families, &mut buffer)?;
    Ok(String::from_utf8(buffer)?)
}

/// Record HTTP request
pub fn record_http_request(method: &str, endpoint: &str, status: u16) {
    HTTP_REQUESTS_TOTAL
        .with_label_values(&[method, endpoint, &status.to_string()])
        .inc();
}

/// Record error
pub fn record_error(error_type: &str, server: Option<&str>) {
    ERRORS_TOTAL
        .with_label_values(&[error_type, server.unwrap_or("unknown")])
        .inc();
}

/// Update session count
pub fn set_active_sessions(count: usize) {
    ACTIVE_SESSIONS.set(count as f64);
}

/// Update MCP server count
pub fn set_mcp_servers_up(count: usize) {
    MCP_SERVER_UP.set(count as f64);
}

/// Update message buffer size
pub fn set_message_buffer_size(size: usize) {
    MESSAGE_BUFFER_SIZE.set(size as f64);
}

/// Record session creation
pub fn record_session_created(transport: &str) {
    TOTAL_SESSIONS_CREATED
        .with_label_values(&[transport])
        .inc();
}

/// Record message replay
pub fn record_message_replay(session_id: &str, count: usize) {
    MESSAGE_REPLAY_TOTAL
        .with_label_values(&[session_id])
        .inc_by(count as f64);
}

/// Update active connections
pub fn set_active_connections(count: usize) {
    ACTIVE_CONNECTIONS.set(count as f64);
}

/// Record WebSocket connection
pub fn record_websocket_connection(status: &str) {
    WEBSOCKET_CONNECTIONS_TOTAL
        .with_label_values(&[status])
        .inc();
}
