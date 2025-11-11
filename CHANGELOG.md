# Changelog

All notable changes to MCP Citadel will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - 2025-01-11

### 🎉 Initial Release

The Citadel - A production-ready MCP server router built in Rust.

### Added

#### Core Features
- **Unix Socket Router** - Central hub for routing MCP messages from multiple clients to shared servers
- **Client Adapter** (`mcp-client`) - Transparent 619KB proxy binary for seamless client integration
- **Server Management** - Automatic spawning and lifecycle management of MCP servers from Claude config

#### Robustness
- **Double-Start Prevention** - Blocks multiple instances with PID file checking
- **Immediate Crash Detection** - Identifies config errors (crashes <5s) and skips retry
- **Smart Restart Limits** - Max 3 restart attempts for runtime failures
- **Graceful Shutdown** - SIGTERM/SIGINT handlers with complete cleanup
- **Health Monitoring** - 30-second health checks with auto-restart
- **Startup Validation** - 100ms validation period with stderr capture

#### Operations
- **Daemon Mode** - Background process management with PID tracking
- **Log File Support** - `--log-file` flag for persistent logging
- **Status Tracking** - JSON status file with uptime and server count
- **Auto-Start Scripts** - macOS launchd service installation

#### Security
- **Socket Permissions** - Unix socket set to 0600 (owner only)
- **Environment Isolation** - Proper env var inheritance (parent + config)

### Performance

- **67% Memory Reduction** - From 54 processes (3 clients × 18 servers) to 19 processes (1 hub + 18 servers)
  - Before: ~5.4GB memory usage
  - After: ~1.8GB memory usage
  - **Savings: 3.6GB**

- **10x Faster Startup** - Clients connect to already-running servers
  - Before: 50-100ms per client (spawning servers)
  - After: <5ms (socket connection only)

- **Binary Sizes**
  - Hub: 1.3MB
  - Client: 619KB

### Technical Details

- **Language**: Rust 2021 edition
- **Async Runtime**: Tokio with full features
- **Protocol**: JSON-RPC 2.0 (MCP specification)
- **Transport**: Unix domain sockets
- **Platforms**: macOS (tested), Linux (should work)

### Documentation

- `README.md` - Quick start and overview
- `PRODUCTION.md` - Deployment guide with client integration
- `ROBUSTNESS.md` - All protections + future roadmap
- `IMPLEMENTATION.md` - Technical architecture details

### Known Issues

- Some MCP servers (e.g., Polar, obsidian-mcp-tools) may crash due to config/dependency issues
- These are handled gracefully with clear error messages

### Migration from Direct MCP Servers

Before (Claude config):
```json
{
  "mcpServers": {
    "github": {
      "command": "npx",
      "args": ["-y", "@modelcontextprotocol/server-github"]
    }
  }
}
```

After (via Citadel):
```json
{
  "mcpServers": {
    "github": {
      "command": "mcp-client",
      "args": ["github"]
    }
  }
}
```

### Installation

```bash
# Install
./install.sh

# Start hub
mcp-citadel start

# List servers
mcp-citadel servers

# Check status
mcp-citadel status
```

### Contributors

- Kevin Hill ([@kivo360](https://github.com/kivo360))

---

## [Unreleased]

### Planned Features (v0.2.0)

See `ROBUSTNESS.md` for full roadmap:
- Health check pings (detect hung processes)
- Circuit breaker pattern
- Config validation before start
- Hot reload on config changes
- Metrics endpoint
- Resource limits per server

[0.1.0]: https://github.com/kivo360/mcp-citadel/releases/tag/v0.1.0
