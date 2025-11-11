# HTTP/SSE Transport for MCP Citadel

MCP Citadel now supports **Streamable HTTP** transport as defined in the [MCP specification 2025-06-18](https://modelcontextprotocol.io/specification/2025-06-18/basic/transports#streamable-http), enabling remote access to your MCP servers over HTTP.

## What is Streamable HTTP?

Streamable HTTP is the modern MCP transport protocol that replaces the deprecated HTTP+SSE transport. It provides:

- **Single endpoint** (`/mcp`) for all communication
- **Bidirectional messaging** via POST (client→server) and GET (server→client SSE)
- **Session management** with `Mcp-Session-Id` header
- **Protocol versioning** via `MCP-Protocol-Version` header
- **Resumability** with `Last-Event-ID` for broken connections
- **Security** through Origin validation and localhost binding

## Quick Start

### Enable HTTP Transport

```bash
# Start with HTTP transport enabled
mcp-citadel start --foreground --enable-http

# Custom port and host
mcp-citadel start --foreground --enable-http --http-port 8080 --http-host 127.0.0.1
```

Your HTTP server will be available at `http://127.0.0.1:3000/mcp` (or your configured port).

### Test with curl

```bash
# POST an initialize request
curl -X POST http://127.0.0.1:3000/mcp \
  -H "Content-Type: application/json" \
  -H "MCP-Protocol-Version: 2025-06-18" \
  -d '{
    "jsonrpc": "2.0",
    "id": 1,
    "method": "initialize",
    "params": {
      "protocolVersion": "2025-06-18",
      "capabilities": {},
      "clientInfo": {
        "name": "test-client",
        "version": "1.0.0"
      }
    }
  }'

# The response will include a Mcp-Session-Id header
# Use this session ID for subsequent requests
```

## Architecture

MCP Citadel runs **dual transports** simultaneously:

```
┌─────────────────────────────────────────┐
│            MCP Citadel                  │
│                                         │
│  ┌──────────────┐   ┌───────────────┐  │
│  │ Unix Socket  │   │ HTTP Server   │  │
│  │ /tmp/mcp-    │   │ :3000/mcp     │  │
│  │ citadel.sock │   │               │  │
│  └──────┬───────┘   └───────┬───────┘  │
│         │                   │          │
│         └────────┬──────────┘          │
│                  │                     │
│          ┌───────▼────────┐            │
│          │  HubManager    │            │
│          │  (Shared Core) │            │
│          └───────┬────────┘            │
│                  │                     │
│     ┌────────────┼────────────┐        │
│     │            │            │        │
│ ┌───▼───┐   ┌───▼───┐   ┌───▼───┐    │
│ │GitHub │   │Fetch  │   │...    │    │
│ └───────┘   └───────┘   └───────┘    │
└─────────────────────────────────────────┘
```

## Protocol Flow

### Initialization (POST)

```http
POST /mcp HTTP/1.1
Host: 127.0.0.1:3000
Content-Type: application/json
MCP-Protocol-Version: 2025-06-18

{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "initialize",
  "params": { ... }
}
```

**Response:**
```http
HTTP/1.1 200 OK
Content-Type: application/json
Mcp-Session-Id: 550e8400-e29b-41d4-a716-446655440000

{
  "jsonrpc": "2.0",
  "id": 1,
  "result": { ... }
}
```

### Subsequent Requests (POST)

All subsequent messages **MUST** include the session ID:

```http
POST /mcp HTTP/1.1
Host: 127.0.0.1:3000
Content-Type: application/json
MCP-Protocol-Version: 2025-06-18
Mcp-Session-Id: 550e8400-e29b-41d4-a716-446655440000

{
  "jsonrpc": "2.0",
  "id": 2,
  "method": "tools/list",
  "params": { "server": "github" }
}
```

### SSE Stream (GET)

Open a server-sent events stream to receive server-initiated messages:

```http
GET /mcp HTTP/1.1
Host: 127.0.0.1:3000
Accept: text/event-stream
Mcp-Session-Id: 550e8400-e29b-41d4-a716-446655440000
```

**Response:**
```http
HTTP/1.1 200 OK
Content-Type: text/event-stream

id: 1
data: {"jsonrpc":"2.0","method":"notifications/progress","params":{...}}

id: 2
data: {"jsonrpc":"2.0","id":2,"result":{...}}
```

### Resumability

If your connection drops, resume using `Last-Event-ID`:

```http
GET /mcp HTTP/1.1
Host: 127.0.0.1:3000
Accept: text/event-stream
Mcp-Session-Id: 550e8400-e29b-41d4-a716-446655440000
Last-Event-ID: 5
```

The server will replay messages after event ID 5.

## Security

### Built-in Protections

MCP Citadel implements critical security measures:

1. **Localhost-only binding** (default: `127.0.0.1`)
   - Prevents external network access
   - Override with `--http-host` only if needed

2. **Origin validation**
   - Blocks non-localhost origins by default
   - Prevents DNS rebinding attacks

3. **Session management**
   - UUIDs for session IDs
   - 1-hour session timeout (configurable)
   - Automatic cleanup of expired sessions

### Security Recommendations

#### For Development

```bash
# Default: localhost only
mcp-citadel start --foreground --enable-http
```

#### For Production

**Do NOT expose to public internet without:**

1. **Reverse proxy with TLS**
   ```nginx
   server {
       listen 443 ssl http2;
       server_name mcp.example.com;
       
       ssl_certificate /path/to/cert.pem;
       ssl_certificate_key /path/to/key.pem;
       
       location /mcp {
           proxy_pass http://127.0.0.1:3000/mcp;
           proxy_http_version 1.1;
           proxy_set_header Upgrade $http_upgrade;
           proxy_set_header Connection "upgrade";
           proxy_set_header Host $host;
           proxy_set_header X-Real-IP $remote_addr;
       }
   }
   ```

2. **Authentication** (future: OAuth, API keys)
   - Currently NOT implemented
   - Use network-level auth (VPN, firewall)

3. **Rate limiting** (future feature)
   - Implement at proxy level for now

### DNS Rebinding Attack Prevention

MCP Citadel validates the `Origin` header to prevent malicious websites from accessing your local MCP server.

**Allowed origins:**
- `http://localhost:*`
- `http://127.0.0.1:*`
- `null` (for testing)

**Blocked:**
- Any external domain
- Any non-localhost origin

## Configuration Options

### CLI Flags

```bash
mcp-citadel start --foreground \
  --enable-http \
  --http-port 3000 \
  --http-host 127.0.0.1
```

### Environment Variables (Future)

Coming soon: Configuration via environment variables or config file.

## Compatibility

### MCP Clients

Works with any client implementing Streamable HTTP transport:

- ✅ **mcp-remote** adapter (recommended for testing)
- ✅ **Custom HTTP clients** (curl, Postman, etc.)
- ⏳ **Claude Desktop** (stdio only currently)
- ⏳ **Cursor** (stdio only currently)

### Protocol Versions

- **2025-06-18** (primary)
- **2025-03-26** (backwards compatible)

## Troubleshooting

### "Connection refused"

```bash
# Ensure HTTP is enabled
mcp-citadel start --foreground --enable-http
```

### "Origin forbidden"

```bash
# Check your client is using localhost
curl -H "Origin: http://localhost" ...
```

### "Session not found" (404)

```bash
# Session expired or invalid
# Re-initialize to get new session
```

### "Bad request" (400)

```bash
# Missing session ID or invalid JSON
# Ensure Mcp-Session-Id header is present
```

## Testing with mcp-remote

The `mcp-remote` adapter allows stdio-based MCP clients (like Claude Desktop) to connect to HTTP servers:

```bash
# Install mcp-remote
npm install -g mcp-remote

# Start MCP Citadel with HTTP
mcp-citadel start --foreground --enable-http

# Configure Claude Desktop to use mcp-remote
# ~/.config/Claude/claude_desktop_config.json:
{
  "mcpServers": {
    "citadel-github": {
      "command": "mcp-remote",
      "args": ["http://127.0.0.1:3000/mcp", "github"]
    }
  }
}
```

## Roadmap

### Implemented ✅
- Streamable HTTP protocol
- Session management
- Origin validation
- Protocol versioning
- Basic SSE streaming

### Coming Soon 🚀
- Full SSE message replay (resumability)
- OAuth authentication
- Rate limiting
- Custom CORS origins
- TLS support (direct)
- Config file support

## Resources

- [MCP Specification 2025-06-18](https://modelcontextprotocol.io/specification/2025-06-18/basic/transports)
- [Streamable HTTP Transport](https://modelcontextprotocol.io/specification/2025-06-18/basic/transports#streamable-http)
- [MCP Citadel GitHub](https://github.com/kivo360/mcp-citadel)

## Support

Issues? Questions? 

- [GitHub Issues](https://github.com/kivo360/mcp-citadel/issues)
- [MCP Discord](https://discord.gg/mcp-community)
