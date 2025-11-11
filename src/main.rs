mod cli;
mod config;
mod daemon;
mod metrics;
mod router;
mod transport;

use anyhow::Result;
use clap::Parser;
use std::sync::Arc;
use tokio::signal;
use tracing::{info, warn};
use tracing_subscriber;

use cli::{Cli, Commands};
use config::{load_claude_config, load_hub_config};
use router::{HubManager, HubRouter};
use transport::HttpTransport;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Start { foreground, log_file, enable_http, http_port, http_host, message_buffer_size } => {
            if foreground {
                start_hub(foreground, log_file, enable_http, http_port, http_host, message_buffer_size).await?;
            } else {
                daemon::daemonize()?;
            }
        }
        Commands::Stop => {
            daemon::stop()?;
        }
        Commands::Status => {
            let status = daemon::status()?;
            println!("{}", status);
        }
        Commands::Servers => {
            list_servers()?;
        }
    }

    Ok(())
}

async fn start_hub(
    _foreground: bool, 
    log_file: Option<std::path::PathBuf>,
    enable_http: bool,
    http_port: u16,
    http_host: String,
    message_buffer_size: usize,
) -> Result<()> {
    // Check if already running
    if daemon::is_running()? {
        eprintln!("❌ MCP Citadel is already running!");
        eprintln!("   Check status: mcp-citadel status");
        eprintln!("   Stop it:      mcp-citadel stop");
        std::process::exit(1);
    }
    
    // Write PID file immediately
    daemon::write_pid(std::process::id())?;
    
    // Setup logging
    if let Some(log_path) = log_file {
        // Log to file
        use std::sync::{Arc, Mutex};
        use tracing_subscriber::fmt::writer::MakeWriter;
        
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)?;
        
        let file_writer = Arc::new(Mutex::new(file));
        
        // Custom writer wrapper
        struct FileWriter(Arc<Mutex<std::fs::File>>);
        
        impl<'a> MakeWriter<'a> for FileWriter {
            type Writer = std::io::LineWriter<std::fs::File>;
            
            fn make_writer(&'a self) -> Self::Writer {
                let file = self.0.lock().unwrap();
                let fd = file.try_clone().unwrap();
                std::io::LineWriter::new(fd)
            }
        }
        
        tracing_subscriber::fmt()
            .with_target(false)
            .with_level(true)
            .with_writer(FileWriter(file_writer))
            .init();
        
        println!("✓ Logging to: {:?}", log_path);
    } else {
        // Log to stdout
        tracing_subscriber::fmt()
            .with_target(false)
            .with_level(true)
            .init();
    }

    // Load configuration
    let mut hub_config = load_hub_config()?;
    
    // Override HTTP config from CLI flags
    if enable_http {
        if let Some(http_config) = &mut hub_config.http {
            http_config.enabled = true;
            http_config.port = http_port;
            http_config.host = http_host.clone();
            http_config.message_buffer_size = message_buffer_size;
        }
    }
    
    let server_configs = load_claude_config(&hub_config.claude_config_path)?;

    println!("🚀 Starting MCP Citadel...");
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

    // Wrap manager in Arc for sharing
    let manager = Arc::new(manager);

    println!("✓ Router ready on {}", hub_config.socket_path);
    println!("");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("  MCP Citadel is running!");
    println!("  Press Ctrl+C to stop");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("");

    // Start health monitoring task
    let health_manager = Arc::clone(&manager);
    let health_task = tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(30));
        loop {
            interval.tick().await;
            if let Err(e) = health_manager.health_check().await {
                eprintln!("Health check error: {}", e);
            }
            
            // Write status file
            let uptime = health_manager.uptime();
            let count = health_manager.server_count().await;
            if let Err(e) = daemon::write_status(count, uptime) {
                eprintln!("Failed to write status: {}", e);
            }
        }
    });

    // Start Unix socket router in background
    let router_manager = Arc::clone(&manager);
    let socket_path_for_cleanup = hub_config.socket_path.clone();
    let router_task = tokio::spawn(async move {
        let router = HubRouter::new(hub_config.socket_path, router_manager);
        router.start().await
    });

    // Start HTTP transport if enabled
    let http_task = if let Some(http_config) = hub_config.http.clone() {
        if http_config.enabled {
            let http_manager = Arc::clone(&manager);
            Some(tokio::spawn(async move {
                let transport = HttpTransport::new(http_config, http_manager);
                transport.start().await
            }))
        } else {
            None
        }
    } else {
        None
    };

    // Wait for shutdown signal
    if let Some(http) = http_task {
        tokio::select! {
            result = router_task => {
                match result {
                    Ok(Ok(())) => info!("Unix socket router completed"),
                    Ok(Err(e)) => warn!("Unix socket router error: {}", e),
                    Err(e) => warn!("Unix socket router panicked: {}", e),
                }
            }
            result = http => {
                match result {
                    Ok(Ok(())) => info!("HTTP transport completed"),
                    Ok(Err(e)) => warn!("HTTP transport error: {}", e),
                    Err(e) => warn!("HTTP transport panicked: {}", e),
                }
            }
            _ = shutdown_signal() => {
                info!("Shutdown signal received");
            }
        }
    } else {
        tokio::select! {
            result = router_task => {
                match result {
                    Ok(Ok(())) => info!("Unix socket router completed"),
                    Ok(Err(e)) => warn!("Unix socket router error: {}", e),
                    Err(e) => warn!("Unix socket router panicked: {}", e),
                }
            }
            _ = shutdown_signal() => {
                info!("Shutdown signal received");
            }
        }
    }

    // Graceful shutdown
    println!("");
    println!("🛑 Shutting down MCP Citadel...");
    
    // Stop health monitoring
    health_task.abort();
    
    // Stop all servers
    if let Err(e) = manager.stop_all().await {
        warn!("Error stopping servers: {}", e);
    } else {
        println!("✓ All MCP servers stopped");
    }
    
    // Remove socket file
    if let Err(e) = std::fs::remove_file(&socket_path_for_cleanup) {
        warn!("Failed to remove socket file: {}", e);
    } else {
        println!("✓ Socket file removed");
    }
    
    // Remove PID file
    if let Err(e) = daemon::remove_pid() {
        warn!("Failed to remove PID file: {}", e);
    } else {
        println!("✓ PID file removed");
    }
    
    println!("✓ MCP Citadel stopped gracefully");
    println!("");

    Ok(())
}

/// Wait for shutdown signal (Ctrl+C or SIGTERM)
async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
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
