//! CLI module for MCP Hub

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "mcp-hub")]
#[command(about = "MCP Hub - Centralized MCP server management", long_about = None)]
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
    },

    /// Stop the MCP hub
    Stop,

    /// Show hub status
    Status,

    /// List configured MCP servers
    Servers,
}
