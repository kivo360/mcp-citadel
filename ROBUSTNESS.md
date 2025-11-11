# MCP Citadel Robustness Guide

## 🛡️ Current Protections (Implemented)

### 1. Double-Start Prevention
**Problem:** Multiple hub instances could conflict on the same Unix socket.

**Solution:**
- Checks PID file before starting
- Exits immediately with helpful error if already running
- Cleans up stale PID files automatically

```rust
if daemon::is_running()? {
    eprintln!("❌ MCP Citadel is already running!");
    std::process::exit(1);
}
```

**Files:** `src/main.rs`, `src/daemon/mod.rs`

---

### 2. Immediate Crash Detection
**Problem:** Broken MCP servers (wrong CLI version, bad args) would restart infinitely.

**Solution:**
- Detects crashes within 5 seconds = config error
- **Zero retries** for immediate crashes
- Shows helpful diagnostic messages

```rust
let is_immediate_crash = uptime.as_secs() < 5;

if is_immediate_crash {
    error!("Server {} crashed immediately ({:.1}s uptime)", name, uptime);
    error!("This usually means:");
    error!("  • Wrong command or arguments in Claude config");
    error!("  • Missing dependencies (run: npm install -g {command})");
    error!("  • Incompatible CLI version");
    
    // Don't retry - it's a config problem
    servers.remove(&name);
}
```

**Result:** Polar server crashes once, never retries, no spam!

**Files:** `src/router/mod.rs` (health_check function)

---

### 3. Smart Restart Limits
**Problem:** Legitimate crashes (OOM, network issues) should retry, but not forever.

**Solution:**
- Max 3 restart attempts for gradual failures (>5s uptime)
- Tracks restart count per server
- Resets count on successful health check
- Permanent disable after 3 failures

```rust
const MAX_RESTARTS: u32 = 3;

if *count >= MAX_RESTARTS {
    error!("Server {} has crashed {} times. Giving up.", name, count);
    servers.remove(&name);
}
```

**Files:** `src/router/mod.rs`

---

### 4. Startup Validation
**Problem:** Servers might accept spawn but crash during initialization.

**Solution:**
- Waits 100ms after spawn
- Checks if process died
- Captures stderr output
- Returns error with actual message

```rust
tokio::time::sleep(Duration::from_millis(100)).await;

if let Ok(Some(status)) = server.process.try_wait() {
    let mut error_msg = String::new();
    server.stderr.read_line(&mut error_msg).await;
    
    return Err(anyhow!("Crashed immediately: {}", error_msg));
}
```

**Files:** `src/router/mod.rs` (MCPServerProcess::start)

---

### 5. Graceful Shutdown
**Problem:** Ctrl+C or SIGTERM could leave zombie processes and stale files.

**Solution:**
- Signal handlers for SIGTERM and SIGINT
- Stops all MCP servers cleanly
- Removes Unix socket file
- Removes PID file
- Aborts health monitor task

```rust
tokio::select! {
    _ = router_task => { /* router ended */ }
    _ = shutdown_signal() => {
        manager.stop_all().await?;
        fs::remove_file(socket_path)?;
        daemon::remove_pid()?;
    }
}
```

**Result:** Clean exits every time, no orphaned processes

**Files:** `src/main.rs` (shutdown_signal, start_hub)

---

### 6. PID File Management
**Problem:** Stale PID files from crashes prevent restart.

**Solution:**
- Writes PID immediately on start
- Checks if PID is actually running (not just file exists)
- Auto-removes stale PID files
- Cleans up on graceful exit

```rust
pub fn is_running() -> Result<bool> {
    match read_pid() {
        Ok(pid) => {
            match kill(Pid::from_raw(pid), None) {
                Ok(_) => Ok(true),  // Process exists
                Err(_) => {
                    // Stale PID file
                    fs::remove_file(pid_file());
                    Ok(false)
                }
            }
        }
        Err(_) => Ok(false)
    }
}
```

**Files:** `src/daemon/mod.rs`

---

### 7. Stderr Capture
**Problem:** Silent failures - servers crash with no indication why.

**Solution:**
- Captures stderr from all MCP servers
- Shows actual error messages in logs
- Helps diagnose config problems

**Files:** `src/router/mod.rs` (MCPServerProcess struct)

---

### 8. Uptime Tracking
**Problem:** Can't distinguish config errors from runtime issues.

**Solution:**
- Tracks start time for each server
- Calculates uptime on crash
- Different handling for instant crashes vs. gradual failures

```rust
struct MCPServerProcess {
    start_time: std::time::Instant,
    // ...
}

let uptime = server.start_time.elapsed();
// Now we can tell: instant crash (<5s) vs runtime crash (>5s)
```

**Files:** `src/router/mod.rs`

---

## 🔮 Future Protections (Recommended)

### 9. Health Check Pings (High Priority)
**Problem:** Server process exists but is hung/unresponsive.

**Solution:**
```rust
// Send actual MCP ping every 60s
async fn health_ping(server: &mut MCPServerProcess) -> Result<()> {
    let ping = json!({"jsonrpc":"2.0","id":"ping","method":"ping"});
    
    tokio::time::timeout(
        Duration::from_secs(5),
        server.send_receive(&ping)
    ).await??;
    
    Ok(())
}
```

**Benefit:** Detect hung processes that aren't crashed

---

### 10. Circuit Breaker Pattern (High Priority)
**Problem:** Flapping servers (crash, restart, crash, restart...) waste resources.

**Solution:**
```rust
struct CircuitBreaker {
    failures_in_window: u32,
    window_start: Instant,
    state: State, // Closed, Open, HalfOpen
}

// If 5 crashes in 60 seconds → Open (stop trying)
// After 5 minutes → HalfOpen (try once)
// If success → Closed (normal operation)
```

**Benefit:** Prevent resource exhaustion from flapping servers

---

### 11. Resource Limits (Medium Priority)
**Problem:** Runaway servers consuming all CPU/memory.

**Solution:**
```rust
#[cfg(target_os = "macos")]
fn set_resource_limits(cmd: &mut Command) {
    // Max 512MB memory per server
    // Max 50% CPU
    cmd.pre_exec(|| {
        unsafe {
            libc::setrlimit(
                libc::RLIMIT_AS,
                &libc::rlimit { rlim_cur: 512_000_000, rlim_max: 512_000_000 }
            );
        }
        Ok(())
    });
}
```

**Benefit:** One bad server can't take down the whole system

---

### 12. Metrics & Monitoring (Medium Priority)
**Problem:** No visibility into hub health over time.

**Solution:**
```rust
struct HubMetrics {
    total_restarts: HashMap<String, u64>,
    uptime_history: Vec<(String, Duration)>,
    last_crash_times: HashMap<String, SystemTime>,
    messages_routed: u64,
    average_latency: Duration,
}

// Expose via HTTP endpoint or write to metrics file
// GET /metrics → Prometheus format
```

**Benefit:** Understand patterns, predict failures

---

### 13. Automatic Config Validation (High Priority)
**Problem:** Invalid configs only discovered at runtime.

**Solution:**
```rust
pub fn validate_config(config: &ServerConfig) -> Result<()> {
    // Check command exists
    which::which(&config.command)?;
    
    // Check required env vars are set
    for var in ["GITHUB_TOKEN", "API_KEY"] {
        if config.env.get(var).is_none() {
            warn!("Server {} missing env var: {}", config.name, var);
        }
    }
    
    // Test spawn (dry run)
    let output = Command::new(&config.command)
        .args(&["--help"])
        .output()?;
    
    if !output.status.success() {
        return Err(anyhow!("Command failed: {:?}", output.stderr));
    }
    
    Ok(())
}
```

**Benefit:** Catch config errors before starting servers

---

### 14. Rate Limiting (Low Priority)
**Problem:** Malicious/buggy client floods hub with requests.

**Solution:**
```rust
use governor::{Quota, RateLimiter};

struct ClientLimiter {
    limiter: RateLimiter<String, _, _>,
}

// 100 requests per second per client
let quota = Quota::per_second(nonzero!(100u32));
```

**Benefit:** Protect hub from DoS

---

### 15. Hot Reload (Medium Priority)
**Problem:** Config changes require full restart.

**Solution:**
```rust
// Watch Claude config file for changes
use notify::Watcher;

async fn watch_config(hub: Arc<HubManager>) {
    let (tx, rx) = channel();
    let mut watcher = notify::watcher(tx, Duration::from_secs(1))?;
    watcher.watch(config_path, RecursiveMode::NonRecursive)?;
    
    while let Ok(event) = rx.recv() {
        info!("Config changed, reloading...");
        hub.reload_config().await?;
    }
}
```

**Benefit:** Zero-downtime config updates

---

### 16. Structured Logging (Low Priority)
**Problem:** Grep-based log analysis is tedious.

**Solution:**
```rust
use tracing_subscriber::fmt::format::json;

tracing_subscriber::fmt()
    .json()
    .with_span_list(false)
    .init();

// Now logs are machine-parseable
// {"timestamp":"2025-01-11T07:00:00Z","level":"error","server":"polar","uptime":1.2}
```

**Benefit:** Easy analysis with jq, Grafana, etc.

---

### 17. Retry Backoff (Medium Priority)
**Problem:** Restart immediately might hit same issue.

**Solution:**
```rust
let backoff = ExponentialBackoff {
    initial_interval: Duration::from_secs(1),
    max_interval: Duration::from_secs(60),
    multiplier: 2.0,
};

// Wait: 1s, 2s, 4s, 8s, 16s, 32s, 60s, 60s...
tokio::time::sleep(backoff.next()).await;
```

**Benefit:** Give transient issues time to resolve

---

### 18. Crash Dump Collection (Low Priority)
**Problem:** No forensics after crash.

**Solution:**
```rust
async fn capture_crash_dump(server: &MCPServerProcess, status: ExitStatus) {
    let dump_dir = home_dir().unwrap().join(".mcp-citadel/crashes");
    fs::create_dir_all(&dump_dir)?;
    
    let dump_file = dump_dir.join(format!(
        "{}-{}.json",
        server.name,
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs()
    ));
    
    let dump = json!({
        "server": server.name,
        "exit_status": format!("{:?}", status),
        "uptime_seconds": server.start_time.elapsed().as_secs(),
        "command": server.config.command,
        "args": server.config.args,
        "timestamp": chrono::Utc::now().to_rfc3339(),
    });
    
    fs::write(dump_file, serde_json::to_string_pretty(&dump)?)?;
}
```

**Benefit:** Debug crashes after the fact

---

### 19. Unix Socket Permissions (Security - Medium Priority)
**Problem:** Any user can connect to hub socket.

**Solution:**
```rust
use std::os::unix::fs::PermissionsExt;

let listener = UnixListener::bind(socket_path)?;

// Set 0600 permissions (owner only)
let mut perms = fs::metadata(&socket_path)?.permissions();
perms.set_mode(0o600);
fs::set_permissions(&socket_path, perms)?;
```

**Benefit:** Prevent unauthorized access

---

### 20. Message Validation (Security - High Priority)
**Problem:** Malformed MCP messages could crash hub.

**Solution:**
```rust
fn validate_mcp_message(msg: &[u8]) -> Result<MCPMessage> {
    // Size limits
    if msg.len() > 10_000_000 { // 10MB max
        return Err(anyhow!("Message too large"));
    }
    
    // Valid JSON
    let json: Value = serde_json::from_slice(msg)?;
    
    // Required fields
    if json.get("jsonrpc") != Some(&json!("2.0")) {
        return Err(anyhow!("Invalid JSON-RPC version"));
    }
    
    // Valid server name (no path traversal)
    if let Some(server) = json["params"]["server"].as_str() {
        if server.contains("..") || server.contains("/") {
            return Err(anyhow!("Invalid server name"));
        }
    }
    
    Ok(parse_mcp_message(msg)?)
}
```

**Benefit:** Prevent injection attacks, crashes

---

## 📊 Implementation Priority

### Must Have (Before v1.0)
- ✅ Double-start prevention
- ✅ Immediate crash detection
- ✅ Smart restart limits
- ✅ Graceful shutdown
- ⬜ Health check pings
- ⬜ Automatic config validation
- ⬜ Message validation (security)

### Should Have (v1.1)
- ⬜ Circuit breaker pattern
- ⬜ Metrics & monitoring
- ⬜ Resource limits
- ⬜ Retry backoff
- ⬜ Hot reload

### Nice to Have (v2.0+)
- ⬜ Rate limiting
- ⬜ Structured logging
- ⬜ Crash dump collection
- ⬜ Unix socket permissions

---

## 🧪 Testing Each Protection

### Test Double-Start Prevention
```bash
# Terminal 1
mcp-citadel start --foreground

# Terminal 2 (should fail)
mcp-citadel start --foreground
# Expected: "❌ MCP Citadel is already running!"
```

### Test Immediate Crash Detection
```bash
# Add broken server to Claude config
{
  "test-broken": {
    "command": "npx",
    "args": ["nonexistent-package"]
  }
}

# Start hub, should see:
# ERROR Server test-broken crashed immediately (0.1s uptime)
# ERROR This usually means: wrong command or arguments
# (no retries!)
```

### Test Restart Limits
```bash
# Server that crashes after 10s
# Should restart 3 times, then give up
# Watch logs for: "attempt 1/3", "attempt 2/3", "attempt 3/3", "Giving up"
```

### Test Graceful Shutdown
```bash
mcp-citadel start --foreground
# Press Ctrl+C
# Should see:
# ✓ All MCP servers stopped
# ✓ Socket file removed
# ✓ PID file removed
# ✓ MCP Citadel stopped gracefully

# Verify cleanup
ls /tmp/mcp-citadel.sock  # should not exist
cat ~/.mcp-citadel/hub.pid # should not exist
```

---

**Built with 🦀 Rust**  
**Production Ready: January 11, 2025**
