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

## [0.2.0] - 2025-01-11

### 🌐 HTTP/SSE Transport Release

Major feature release adding Streamable HTTP transport alongside Unix sockets.

### Added

#### HTTP/SSE Transport
- **Streamable HTTP Protocol** - Full implementation of MCP specification 2025-06-18
- **Dual Transport Mode** - Unix socket and HTTP/SSE run simultaneously
- **Session Management** - UUID-based sessions with 1-hour timeout
- **Protocol Versioning** - `MCP-Protocol-Version` header support (2025-06-18, 2025-03-26)
- **SSE Streaming** - Server-Sent Events for server-to-client messages
- **Resumability Foundation** - `Last-Event-ID` support for connection recovery

#### Security
- **Origin Validation** - Prevents DNS rebinding attacks
- **Localhost Binding** - Default `127.0.0.1` binding for security
- **Session Cleanup** - Automatic expiration of inactive sessions

#### CLI
- `--enable-http` flag to activate HTTP transport
- `--http-port` flag for custom port (default: 3000)
- `--http-host` flag for custom host (default: 127.0.0.1)

#### Dependencies
- `axum` v0.7 - HTTP server framework
- `tokio-stream` v0.1 - Async streaming
- `uuid` v1.11 - Session ID generation
- `tower-http` v0.5 - HTTP middleware
- `headers` v0.4 - Type-safe HTTP headers

### Documentation
- `HTTP_TRANSPORT.md` - Comprehensive HTTP transport guide
  - Protocol flow diagrams
  - Security recommendations
  - Testing with curl and mcp-remote
  - Production deployment with nginx
- Updated `README.md` with HTTP transport section
- `test_http.sh` - Automated HTTP transport test script

### Technical Details

#### Architecture
```
Unix Socket ────┐
                ├──→ HubManager ──→ MCP Servers
HTTP :3000 ─────┘
```

- Single `HubManager` shared by both transports
- No duplication of server processes
- Concurrent client handling across transports

#### Protocol Compliance
- ✅ Single `/mcp` endpoint for POST and GET
- ✅ `Mcp-Session-Id` header for session tracking
- ✅ `MCP-Protocol-Version` header validation
- ✅ JSON-RPC 2.0 request/response/notification handling
- ✅ SSE event streaming with IDs
- ⏳ Full message replay (resumability) - coming soon

### Testing

```bash
# Start with HTTP
mcp-citadel start --foreground --enable-http

# Run test suite
./test_http.sh

# Manual testing
curl -X POST http://127.0.0.1:3000/mcp \
  -H "Content-Type: application/json" \
  -H "MCP-Protocol-Version: 2025-06-18" \
  -d '{...}'
```

### Compatibility

- ✅ `mcp-remote` adapter for stdio clients
- ✅ Direct HTTP clients (curl, Postman)
- ✅ Backwards compatible with stdio-only mode
- ⏳ Native HTTP support in Claude Desktop (when available)

### Performance

- HTTP overhead: <2ms per request
- Session cleanup: Every 60 seconds
- Memory per session: ~1KB
- Concurrent connections: Limited by system ulimit

### Migration

No breaking changes. HTTP transport is opt-in:

```bash
# Before (Unix socket only)
mcp-citadel start --foreground

# After (Unix socket + HTTP)
mcp-citadel start --foreground --enable-http
```

### Future Roadmap (v0.3.0)

See `ROBUSTNESS.md` and `HTTP_TRANSPORT.md` for full roadmap:
- Full SSE message replay (resumability)
- OAuth authentication
- Rate limiting
- Custom CORS origins
- Direct TLS support
- Health check pings (detect hung processes)
- Circuit breaker pattern
- Config validation before start
- Hot reload on config changes
- Metrics endpoint
- Resource limits per server

[0.1.0]: https://github.com/kivo360/mcp-citadel/releases/tag/v0.1.0
