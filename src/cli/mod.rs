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
    },

    /// Stop the MCP hub
    Stop,

    /// Show hub status
    Status,

    /// List configured MCP servers
    Servers,
}
