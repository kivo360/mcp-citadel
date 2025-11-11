//! CLI module for MCP Citadel

use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "mcp-citadel")]
#[command(about = "MCP Citadel - Centralized MCP server management", long_about = None)]
#[command(version)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Start the MCP hub
    Start {
        /// Run in foreground (don't daemonize)
        #[arg(short, long)]
        foreground: bool,
        
        /// Log file path (default: stdout)
        #[arg(long)]
        log_file: Option<PathBuf>,
        
        /// Enable HTTP transport
        #[arg(long)]
        enable_http: bool,
        
        /// HTTP port (default: 3000)
        #[arg(long, default_value = "3000")]
        http_port: u16,
        
        /// HTTP host (default: 127.0.0.1)
        #[arg(long, default_value = "127.0.0.1")]
        http_host: String,
    },

    /// Stop the MCP hub
    Stop,

    /// Show hub status
    Status,

    /// List configured MCP servers
    Servers,
}
