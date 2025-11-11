//! Daemon module for background process management

use anyhow::{Context, Result};
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};

/// PID file path
fn pid_file() -> PathBuf {
    dirs::home_dir()
        .unwrap()
        .join(".mcp-citadel")
        .join("hub.pid")
}

/// Status file path
fn status_file() -> PathBuf {
    dirs::home_dir()
        .unwrap()
        .join(".mcp-citadel")
        .join("status.json")
}

/// Ensure .mcp-citadel directory exists
fn ensure_dir() -> Result<()> {
    let dir = dirs::home_dir().unwrap().join(".mcp-citadel");
    fs::create_dir_all(&dir)?;
    Ok(())
}

/// Start hub as daemon
pub fn daemonize() -> Result<()> {
    ensure_dir()?;
    
    // Check if already running
    if is_running()? {
        anyhow::bail!("Hub is already running");
    }
    
    // Get current binary path
    let binary = std::env::current_exe()?;
    
    // Spawn detached process
    let child = Command::new(binary)
        .args(&["start", "--foreground"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .context("Failed to spawn daemon process")?;
    
    // Write PID file
    fs::write(pid_file(), child.id().to_string())?;
    
    println!("✓ MCP Citadel started (PID: {})", child.id());
    
    Ok(())
}

/// Stop the daemon
pub fn stop() -> Result<()> {
    let pid = read_pid()?;
    
    // Send SIGTERM
    #[cfg(unix)]
    {
        use nix::sys::signal::{kill, Signal};
        use nix::unistd::Pid;
        
        kill(Pid::from_raw(pid as i32), Signal::SIGTERM)
            .context("Failed to send SIGTERM")?;
    }
    
    // Remove PID file
    let _ = fs::remove_file(pid_file());
    
    println!("✓ MCP Citadel stopped");
    
    Ok(())
}

/// Check if hub is running
pub fn is_running() -> Result<bool> {
    match read_pid() {
        Ok(pid) => {
            // Check if process exists
            #[cfg(unix)]
            {
                use nix::sys::signal::kill;
                use nix::unistd::Pid;
                
                match kill(Pid::from_raw(pid as i32), None) {
                    Ok(_) => Ok(true),
                    Err(_) => {
                        // Process doesn't exist, clean up stale PID file
                        let _ = fs::remove_file(pid_file());
                        Ok(false)
                    }
                }
            }
            
            #[cfg(not(unix))]
            Ok(true)
        }
        Err(_) => Ok(false),
    }
}

/// Read PID from file
fn read_pid() -> Result<u32> {
    let content = fs::read_to_string(pid_file())
        .context("Hub is not running (no PID file)")?;
    
    content.trim().parse()
        .context("Invalid PID file")
}

/// Get hub status
pub fn status() -> Result<String> {
    if !is_running()? {
        return Ok("Hub is not running".to_string());
    }
    
    let pid = read_pid()?;
    
    // Try to read status file
    if let Ok(status_json) = fs::read_to_string(status_file()) {
        if let Ok(status) = serde_json::from_str::<serde_json::Value>(&status_json) {
            return Ok(serde_json::to_string_pretty(&status)?);
        }
    }
    
    Ok(format!("Hub is running (PID: {})", pid))
}

/// Write PID file
pub fn write_pid(pid: u32) -> Result<()> {
    ensure_dir()?;
    fs::write(pid_file(), pid.to_string())?;
    Ok(())
}

/// Remove PID file
pub fn remove_pid() -> Result<()> {
    fs::remove_file(pid_file())
        .context("Failed to remove PID file")
}

/// Write status information
pub fn write_status(server_count: usize, uptime: std::time::Duration) -> Result<()> {
    ensure_dir()?;
    
    let status = serde_json::json!({
        "pid": std::process::id(),
        "server_count": server_count,
        "uptime_seconds": uptime.as_secs(),
        "socket_path": "/tmp/mcp-citadel.sock",
        "timestamp": chrono::Utc::now().to_rfc3339(),
    });
    
    fs::write(status_file(), serde_json::to_string_pretty(&status)?)?;
    
    Ok(())
}
