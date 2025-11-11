//! MCP Citadel Router
//! Routes MCP messages from clients to backend MCP servers

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::process::Stdio;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command};
use tokio::sync::Mutex;
use tracing::{debug, error, info, warn};

use crate::config::ServerConfig;

/// Managed MCP server process
pub struct MCPServerProcess {
    name: String,
    process: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    stderr: BufReader<ChildStderr>,
    start_time: std::time::Instant,
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
        
        // Inherit parent environment and merge with config env
        // This ensures servers have access to PATH, HOME, etc.
        let mut merged_env: HashMap<String, String> = std::env::vars().collect();
        merged_env.extend(config.env.clone());
        
        cmd.args(&config.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .env_clear()
            .envs(&merged_env);

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
        
        let stderr = process
            .stderr
            .take()
            .context("Failed to get stderr")?;

        let stdout = BufReader::new(stdout);
        let stderr = BufReader::new(stderr);

        info!("✓ Started MCP server: {} (PID: {:?})", config.name, process.id());
        
        let mut server = Self {
            name: config.name.clone(),
            process,
            stdin,
            stdout,
            stderr,
            start_time: std::time::Instant::now(),
        };
        
        // Wait 100ms and check if it immediately crashed
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        
        if let Ok(Some(status)) = server.process.try_wait() {
            // Read any error output
            let mut error_msg = String::new();
            let _ = server.stderr.read_line(&mut error_msg).await;
            
            warn!("Server {} crashed during startup: {:?}", config.name, status);
            if !error_msg.is_empty() {
                warn!("Error output: {}", error_msg.trim());
            }
            
            return Err(anyhow::anyhow!(
                "Server crashed immediately with status: {:?}. Error: {}",
                status,
                error_msg.trim()
            ));
        }
        
        Ok(server)
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

/// MCP Citadel Server Manager
pub struct HubManager {
    servers: Arc<Mutex<HashMap<String, MCPServerProcess>>>,
    configs: Vec<ServerConfig>,
    start_time: std::time::Instant,
    restart_counts: Arc<Mutex<HashMap<String, u32>>>,
}

impl HubManager {
    /// Create a new hub manager
    pub async fn new(configs: Vec<ServerConfig>) -> Result<Self> {
        let mut servers = HashMap::new();

        for config in &configs {
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
            configs,
            start_time: std::time::Instant::now(),
            restart_counts: Arc::new(Mutex::new(HashMap::new())),
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

    /// Check health of all servers and restart crashed ones
    pub async fn health_check(&self) -> Result<()> {
        let mut servers = self.servers.lock().await;
        let mut restart_counts = self.restart_counts.lock().await;
        
        const MAX_RESTARTS: u32 = 3;
        
        for config in &self.configs {
            // Check if server exists
            if let Some(server) = servers.get_mut(&config.name) {
                // Check if process is still alive
                match server.process.try_wait() {
                    Ok(Some(status)) => {
                        let uptime = server.start_time.elapsed();
                        let count = restart_counts.entry(config.name.clone()).or_insert(0);
                        
                        // Immediate crash detection (< 5 seconds)
                        let is_immediate_crash = uptime.as_secs() < 5;
                        
                        if is_immediate_crash {
                            error!(
                                "Server {} crashed immediately ({:.1}s uptime) with status: {:?}",
                                config.name, uptime.as_secs_f32(), status
                            );
                            error!("This usually means:");
                            error!("  • Wrong command or arguments in Claude config");
                            error!("  • Missing dependencies (run: npm install -g {})", config.command);
                            error!("  • Incompatible CLI version");
                            error!("Command: {} {:?}", config.command, config.args);
                            
                            // Don't retry immediate crashes - they're config errors
                            servers.remove(&config.name);
                            continue;
                        }
                        
                        if *count >= MAX_RESTARTS {
                            error!(
                                "Server {} has crashed {} times. Giving up. Check your Claude config.",
                                config.name, count
                            );
                            servers.remove(&config.name);
                            continue;
                        }
                        
                        warn!("Server {} exited after {:.1}s with status: {:?}", config.name, uptime.as_secs_f32(), status);
                        *count += 1;
                        
                        // Restart the server
                        info!("Restarting server: {} (attempt {}/{})", config.name, count, MAX_RESTARTS);
                        match MCPServerProcess::start(config.clone()).await {
                            Ok(new_server) => {
                                servers.insert(config.name.clone(), new_server);
                                info!("✓ Restarted server: {}", config.name);
                            }
                            Err(e) => {
                                error!("Failed to restart server {}: {}", config.name, e);
                            }
                        }
                    }
                    Ok(None) => {
                        // Still running, all good
                        // Reset restart count on successful health check
                        restart_counts.insert(config.name.clone(), 0);
                    }
                    Err(e) => {
                        error!("Error checking server {}: {}", config.name, e);
                    }
                }
            }
        }
        
        Ok(())
    }

    /// Get uptime
    pub fn uptime(&self) -> std::time::Duration {
        self.start_time.elapsed()
    }

    /// Get server count
    pub async fn server_count(&self) -> usize {
        let servers = self.servers.lock().await;
        servers.len()
    }
}

/// MCP Citadel Router - Unix socket server
pub struct HubRouter {
    socket_path: String,
    manager: Arc<HubManager>,
}

impl HubRouter {
    /// Create a new router
    pub fn new(socket_path: String, manager: Arc<HubManager>) -> Self {
        Self {
            socket_path,
            manager,
        }
    }

    /// Start the router
    pub async fn start(&self) -> Result<()> {
        // Remove existing socket
        let _ = std::fs::remove_file(&self.socket_path);

        let listener = UnixListener::bind(&self.socket_path)
            .context("Failed to bind Unix socket")?;
        
        // Set socket permissions to 0600 (owner only) for security
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&self.socket_path)?.permissions();
            perms.set_mode(0o600);
            std::fs::set_permissions(&self.socket_path, perms)?;
        }

        info!("🚀 MCP Citadel listening on {}", self.socket_path);

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
