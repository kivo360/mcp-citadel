//! MCP Hub Router
//! Routes MCP messages from clients to backend MCP servers

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::process::Stdio;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::Mutex;
use tracing::{debug, error, info, warn};

use crate::config::ServerConfig;

/// Managed MCP server process
pub struct MCPServerProcess {
    name: String,
    process: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

impl MCPServerProcess {
    /// Start an MCP server process
    pub async fn start(config: ServerConfig) -> Result<Self> {
        info!("Starting MCP server: {}", config.name);
        debug!(
            "Command: {} {:?}",
            config.command,
            config.args
        );

        let mut cmd = Command::new(&config.command);
        cmd.args(&config.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .envs(&config.env);

        let mut process = cmd
            .spawn()
            .context(format!("Failed to spawn server: {}", config.name))?;

        let stdin = process
            .stdin
            .take()
            .context("Failed to get stdin")?;

        let stdout = process
            .stdout
            .take()
            .context("Failed to get stdout")?;

        let stdout = BufReader::new(stdout);

        info!("✓ Started MCP server: {} (PID: {:?})", config.name, process.id());

        Ok(Self {
            name: config.name,
            process,
            stdin,
            stdout,
        })
    }

    /// Send a message and receive response
    pub async fn send_receive(&mut self, message: &[u8]) -> Result<Vec<u8>> {
        // Write message
        self.stdin.write_all(message).await?;
        self.stdin.flush().await?;

        // Read response (one line)
        let mut response = Vec::new();
        self.stdout.read_until(b'\n', &mut response).await?;

        Ok(response)
    }

    /// Stop the server
    pub async fn stop(&mut self) -> Result<()> {
        info!("Stopping MCP server: {}", self.name);
        self.process.kill().await?;
        self.process.wait().await?;
        Ok(())
    }
}

/// MCP Hub Server Manager
pub struct HubManager {
    servers: Arc<Mutex<HashMap<String, MCPServerProcess>>>,
}

impl HubManager {
    /// Create a new hub manager
    pub async fn new(configs: Vec<ServerConfig>) -> Result<Self> {
        let mut servers = HashMap::new();

        for config in configs {
            match MCPServerProcess::start(config.clone()).await {
                Ok(server) => {
                    servers.insert(config.name.clone(), server);
                }
                Err(e) => {
                    error!("Failed to start server {}: {}", config.name, e);
                }
            }
        }

        Ok(Self {
            servers: Arc::new(Mutex::new(servers)),
        })
    }

    /// Route a message to a specific server
    pub async fn route_message(&self, server_name: &str, message: &[u8]) -> Result<Vec<u8>> {
        let mut servers = self.servers.lock().await;
        let server = servers
            .get_mut(server_name)
            .context(format!("Server not found: {}", server_name))?;

        server.send_receive(message).await
    }

    /// List all servers
    pub async fn list_servers(&self) -> Vec<String> {
        let servers = self.servers.lock().await;
        servers.keys().cloned().collect()
    }

    /// Stop all servers
    pub async fn stop_all(&self) -> Result<()> {
        let mut servers = self.servers.lock().await;
        for (_name, server) in servers.iter_mut() {
            if let Err(e) = server.stop().await {
                error!("Error stopping server: {}", e);
            }
        }
        Ok(())
    }
}

/// MCP Hub Router - Unix socket server
pub struct HubRouter {
    socket_path: String,
    manager: Arc<HubManager>,
}

impl HubRouter {
    /// Create a new router
    pub fn new(socket_path: String, manager: HubManager) -> Self {
        Self {
            socket_path,
            manager: Arc::new(manager),
        }
    }

    /// Start the router
    pub async fn start(&self) -> Result<()> {
        // Remove existing socket
        let _ = std::fs::remove_file(&self.socket_path);

        let listener = UnixListener::bind(&self.socket_path)
            .context("Failed to bind Unix socket")?;

        info!("🚀 MCP Hub listening on {}", self.socket_path);

        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    let manager = Arc::clone(&self.manager);
                    tokio::spawn(async move {
                        if let Err(e) = handle_client(stream, manager).await {
                            error!("Client error: {}", e);
                        }
                    });
                }
                Err(e) => {
                    error!("Accept error: {}", e);
                }
            }
        }
    }
}

/// Handle a client connection
async fn handle_client(stream: UnixStream, manager: Arc<HubManager>) -> Result<()> {
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    let mut server_name: Option<String> = None;

    loop {
        let mut line = Vec::new();
        let n = reader.read_until(b'\n', &mut line).await?;

        if n == 0 {
            debug!("Client disconnected");
            break;
        }

        // Parse JSON to extract server name
        if server_name.is_none() {
            server_name = extract_server_name(&line);
        }

        match &server_name {
            Some(name) => {
                // Route to backend server
                match manager.route_message(name, &line).await {
                    Ok(response) => {
                        writer.write_all(&response).await?;
                    }
                    Err(e) => {
                        error!("Routing error: {}", e);
                        // Send error response
                        let error_response = format!(
                            "{{\"jsonrpc\":\"2.0\",\"id\":null,\"error\":{{\"code\":-32603,\"message\":\"{}\"}}}}\n",
                            e
                        );
                        writer.write_all(error_response.as_bytes()).await?;
                    }
                }
            }
            None => {
                warn!("No server name specified in message");
                let error_response = "{\"jsonrpc\":\"2.0\",\"id\":null,\"error\":{\"code\":-32602,\"message\":\"Server name not specified\"}}\n";
                writer.write_all(error_response.as_bytes()).await?;
            }
        }
    }

    Ok(())
}

/// Extract server name from MCP message
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
