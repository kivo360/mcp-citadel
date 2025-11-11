# MCP Citadel v0.4.0 - Smart Responses & Bidirectional Communication

## 🚀 What's New

### 1. Smart Response Mode
**Automatic JSON vs SSE selection** - The HTTP transport now intelligently chooses between direct JSON responses and SSE streaming based on the method:

- ✅ **JSON** (instant): `tools/list`, `resources/list`, `prompts/list`, etc.
- ✅ **SSE** (streaming): `initialize`, `sampling/createMessage`, `notifications/*`

**Performance**: 0ms overhead for simple operations!

```bash
# Simple method - returns JSON immediately
curl -X POST http://127.0.0.1:3000/mcp \
  -H "Mcp-Session-Id: abc123" \
  -d '{"method":"tools/list"}'
# → {"jsonrpc":"2.0","result":{...}}

# Streaming method - returns SSE
curl -N -X POST http://127.0.0.1:3000/mcp \
  -d '{"method":"initialize"}'
# → event: session
# → data: {"sessionId":"..."}
```

### 2. Message Replay & Resumability
**Never miss a message** - HTTP sessions now buffer the last 100 messages with event IDs:

- Last-Event-ID header support
- Automatic replay on reconnection
- Perfect for handling temporary disconnections

```bash
# Client reconnects with Last-Event-ID
curl -N -X GET http://127.0.0.1:3000/mcp \
  -H "Mcp-Session-Id: abc123" \
  -H "Last-Event-ID: 42"
# → Replays events 43, 44, 45... automatically
```

### 3. Bidirectional SSE Communication
**Server can now initiate requests** - MCP servers can send notifications and requests to HTTP clients:

- Server-initiated requests (e.g., LLM sampling)
- Real-time notifications (progress, resource changes)
- Event types: `data`, `error`, `notification`, `request`

**Use Cases:**
- Progress updates for long operations
- Resource change notifications
- LLM sampling requests from server

See [`BIDIRECTIONAL_SSE.md`](https://github.com/kivo360/mcp-citadel/blob/master/BIDIRECTIONAL_SSE.md) for complete documentation.

### 4. Enhanced Error Handling
**Structured error responses** with categorization:

- `-32001`: `server_not_found`
- `-32002`: `timeout`
- `-32003`: `server_crash`
- `-32603`: `internal_error`
- `-32700`: `parse_error`

```json
{
  "jsonrpc": "2.0",
  "error": {
    "code": -32001,
    "message": "Server not found: unknown_server",
    "data": {
      "type": "server_not_found",
      "server": "unknown_server"
    }
  }
}
```

## ⚠️ Breaking Changes

**POST /mcp response behavior changed:**
- **Before:** All responses via SSE
- **After:** JSON for simple methods, SSE for streaming methods

**Migration:**
```javascript
// Check Content-Type to determine response type
const response = await fetch('/mcp', {method: 'POST', body: request});
if (response.headers.get('content-type').includes('text/event-stream')) {
  // SSE stream
  const reader = response.body.getReader();
} else {
  // JSON response
  const data = await response.json();
}
```

## 📊 Performance

- **Simple operations:** 0ms SSE overhead (direct JSON)
- **Streaming operations:** Same as v0.3.0 (~2ms)
- **Message replay:** <10ms for 100 buffered messages
- **Binary size:** 1.9MB (920KB compressed)

## 🧪 Testing

Run the included test script:
```bash
./test_v0.4.0.sh
```

## 📚 Documentation

- **New:** [`BIDIRECTIONAL_SSE.md`](https://github.com/kivo360/mcp-citadel/blob/master/BIDIRECTIONAL_SSE.md) - Bidirectional communication guide
- **Updated:** [`CHANGELOG.md`](https://github.com/kivo360/mcp-citadel/blob/master/CHANGELOG.md) - Full v0.4.0 details

## 🎯 What's Next (v0.5.0)

- [ ] Configurable buffer size
- [ ] Automatic client reconnection
- [ ] WebSocket transport option
- [ ] Metrics endpoint (`/metrics`)
- [ ] Health check endpoint (`/health`)

---

## Installation

```bash
# Download binary
wget https://github.com/kivo360/mcp-citadel/releases/download/v0.4.0/mcp-citadel-v0.4.0-macos-arm64.tar.gz

# Extract
tar xzf mcp-citadel-v0.4.0-macos-arm64.tar.gz

# Move to PATH
sudo mv mcp-citadel /usr/local/bin/

# Start with HTTP transport
mcp-citadel start --foreground --enable-http
```

## Full Changelog

See [`CHANGELOG.md`](https://github.com/kivo360/mcp-citadel/blob/master/CHANGELOG.md#040---2025-01-11) for complete details.
