mod cli;
mod config;
mod router;

use anyhow::Result;
use clap::Parser;
use tracing_subscriber;

use cli::{Cli, Commands};
use config::{load_claude_config, load_hub_config};
use router::{HubManager, HubRouter};

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Start { foreground } => {
            start_hub(foreground).await?;
        }
        Commands::Stop => {
            println!("Stopping hub...");
            // TODO: Implement graceful shutdown via PID file
        }
        Commands::Status => {
            println!("Hub status:");
            // TODO: Query running hub
        }
        Commands::Servers => {
            list_servers()?;
        }
    }

    Ok(())
}

async fn start_hub(foreground: bool) -> Result<()> {
    // Setup logging
    tracing_subscriber::fmt()
        .with_target(false)
        .with_level(true)
        .init();

    // Load configuration
    let hub_config = load_hub_config()?;
    let server_configs = load_claude_config(&hub_config.claude_config_path)?;

    println!("🚀 Starting MCP Hub...");
    println!("   Loaded {} MCP servers from Claude config", server_configs.len());
    println!("");

    // Create hub manager and start all servers
    let manager = HubManager::new(server_configs).await?;

    let server_list = manager.list_servers().await;
    println!("✓ Started {} servers:", server_list.len());
    for server in &server_list {
        println!("  • {}", server);
    }
    println!("");

    // Create router
    let router = HubRouter::new(hub_config.socket_path.clone(), manager);

    println!("✓ Router ready on {}", hub_config.socket_path);
    println!("");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("  MCP Hub is running!");
    println!("  Press Ctrl+C to stop");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("");

    // Start router (runs forever)
    router.start().await?;

    Ok(())
}

fn list_servers() -> Result<()> {
    let hub_config = load_hub_config()?;
    let server_configs = load_claude_config(&hub_config.claude_config_path)?;

    println!("");
    println!("📋 Configured MCP Servers:");
    println!("");

    for config in server_configs {
        println!("  {} - {} {:?}", 
            config.name, 
            config.command,
            config.args
        );
    }

    println!("");
    Ok(())
}
