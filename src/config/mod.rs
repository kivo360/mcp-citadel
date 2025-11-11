//! Configuration module for MCP Hub
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
}

impl Default for HubConfig {
    fn default() -> Self {
        let home = dirs::home_dir().expect("Could not find home directory");
        Self {
            socket_path: "/tmp/mcp-hub.sock".to_string(),
            log_level: "info".to_string(),
            claude_config_path: home
                .join("Library/Application Support/Claude/claude_desktop_config.json"),
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
    // Later: load from ~/.mcp-hub/config.toml
    Ok(HubConfig::default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = HubConfig::default();
        assert_eq!(config.socket_path, "/tmp/mcp-hub.sock");
        assert_eq!(config.log_level, "info");
    }
}
