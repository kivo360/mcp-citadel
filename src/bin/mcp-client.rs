//! MCP Client Adapter
//! 
//! Transparent proxy that connects to MCP Citadel and automatically
//! routes messages to the specified server.
//!
//! Usage:
//!   mcp-client <server-name>
//!
//! Example in Claude config:
//!   {
//!     "mcpServers": {
//!       "github": {
//!         "command": "mcp-client",
//!         "args": ["github"]
//!       }
//!     }
//!   }

use anyhow::{Context, Result};
use std::env;
use tokio::io::{self, AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

#[tokio::main]
async fn main() -> Result<()> {
    // Get server name from args
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        eprintln!("Usage: mcp-client <server-name>");
        eprintln!("Example: mcp-client github");
        std::process::exit(1);
    }
    
    let server_name = &args[1];
    
    // Connect to hub
    let hub_socket = "/tmp/mcp-citadel.sock";
    let mut stream = UnixStream::connect(hub_socket)
        .await
        .context("Failed to connect to MCP Citadel. Is it running?")?;
    
    let (hub_read, mut hub_write) = stream.split();
    let mut hub_reader = BufReader::new(hub_read);
    
    // Setup stdio
    let stdin = io::stdin();
    let mut stdin_reader = BufReader::new(stdin);
    let mut stdout = io::stdout();
    
    // Bidirectional forwarding
    let mut stdin_line = String::new();
    let mut hub_line = Vec::new();
    
    loop {
        tokio::select! {
            // Read from stdin (client) → forward to hub
            result = stdin_reader.read_line(&mut stdin_line) => {
                match result {
                    Ok(0) => break, // EOF
                    Ok(_) => {
                        // Parse JSON and inject server name
                        if let Ok(mut json) = serde_json::from_str::<serde_json::Value>(&stdin_line) {
                            // Add server name to params
                            if let Some(obj) = json.as_object_mut() {
                                let params = obj.entry("params")
                                    .or_insert_with(|| serde_json::json!({}));
                                
                                if let Some(params_obj) = params.as_object_mut() {
                                    params_obj.insert("server".to_string(), serde_json::json!(server_name));
                                }
                            }
                            
                            // Forward modified message to hub
                            let modified = serde_json::to_string(&json)?;
                            hub_write.write_all(modified.as_bytes()).await?;
                            hub_write.write_all(b"\n").await?;
                            hub_write.flush().await?;
                        } else {
                            // Forward as-is if not valid JSON
                            hub_write.write_all(stdin_line.as_bytes()).await?;
                            hub_write.flush().await?;
                        }
                        
                        stdin_line.clear();
                    }
                    Err(e) => {
                        eprintln!("stdin error: {}", e);
                        break;
                    }
                }
            }
            
            // Read from hub → forward to stdout (client)
            result = hub_reader.read_until(b'\n', &mut hub_line) => {
                match result {
                    Ok(0) => break, // Hub disconnected
                    Ok(_) => {
                        stdout.write_all(&hub_line).await?;
                        stdout.flush().await?;
                        hub_line.clear();
                    }
                    Err(e) => {
                        eprintln!("hub error: {}", e);
                        break;
                    }
                }
            }
        }
    }
    
    Ok(())
}
