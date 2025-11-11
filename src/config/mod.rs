//! Configuration module for MCP Citadel
//! Loads server configurations from Claude Desktop config

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Hub configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HubConfig {
    /// Unix socket path for the hub
    pub socket_path: String,
    /// Log level
    pub log_level: String,
    /// Path to Claude Desktop config
    pub claude_config_path: PathBuf,
    /// HTTP transport configuration (optional)
    pub http: Option<HttpConfig>,
}

/// HTTP transport configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpConfig {
    /// Enable HTTP transport
    pub enabled: bool,
    /// Host to bind to (default: 127.0.0.1 for security)
    pub host: String,
    /// Port to listen on
    pub port: u16,
    /// Session timeout in seconds
    pub session_timeout_secs: u64,
}

impl Default for HubConfig {
    fn default() -> Self {
        let home = dirs::home_dir().expect("Could not find home directory");
        Self {
            socket_path: "/tmp/mcp-citadel.sock".to_string(),
            log_level: "info".to_string(),
            claude_config_path: home
                .join("Library/Application Support/Claude/claude_desktop_config.json"),
            http: Some(HttpConfig::default()),
        }
    }
}

impl Default for HttpConfig {
    fn default() -> Self {
        Self {
            enabled: false, // Disabled by default for security
            host: "127.0.0.1".to_string(),
            port: 3000,
            session_timeout_secs: 3600, // 1 hour
        }
    }
}

/// Claude Desktop config structure
#[derive(Debug, Deserialize)]
struct ClaudeConfig {
    #[serde(rename = "mcpServers")]
    mcp_servers: HashMap<String, ServerDefinition>,
}

/// MCP server definition from Claude config
#[derive(Debug, Deserialize)]
struct ServerDefinition {
    command: String,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    env: HashMap<String, String>,
}

/// Processed server configuration
#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
    pub env: HashMap<String, String>,
}

/// Load Claude Desktop MCP server configurations
pub fn load_claude_config(path: &Path) -> Result<Vec<ServerConfig>> {
    let content = std::fs::read_to_string(path)
        .context(format!("Failed to read Claude config at {:?}", path))?;

    let claude_config: ClaudeConfig = serde_json::from_str(&content)
        .context("Failed to parse Claude config JSON")?;

    let configs: Vec<ServerConfig> = claude_config
        .mcp_servers
        .into_iter()
        .map(|(name, def)| ServerConfig {
            name,
            command: def.command,
            args: def.args,
            env: def.env,
        })
        .collect();

    Ok(configs)
}

/// Load hub configuration
pub fn load_hub_config() -> Result<HubConfig> {
    // For now, just use defaults
    // Later: load from ~/.mcp-citadel/config.toml
    Ok(HubConfig::default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = HubConfig::default();
        assert_eq!(config.socket_path, "/tmp/mcp-citadel.sock");
        assert_eq!(config.log_level, "info");
    }
}
