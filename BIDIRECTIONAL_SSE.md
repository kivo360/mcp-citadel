# Bidirectional SSE in MCP Citadel

## Overview

Starting with v0.4.0, MCP Citadel supports **bidirectional communication** through Server-Sent Events (SSE). This allows the MCP server to send unsolicited notifications and requests to HTTP clients, not just responses to client requests.

## Architecture

### Session-Based Communication Channel

Each HTTP session maintains a persistent SSE channel (`event_tx`) that can be used for bidirectional communication:

```rust
struct HttpSession {
    event_tx: Option<mpsc::Sender<Result<Event, Infallible>>>,
    // ... other fields
}
```

This channel is:
- Created when POST /mcp is called with a streaming method
- Persisted in the session for the duration of the connection
- Accessible to MCP server processes for sending notifications

### Message Flow

#### Client → Server (Request)
```
HTTP POST /mcp
  ↓
[Routing Layer]
  ↓
MCP Server Process
```

#### Server → Client (Response/Notification)
```
MCP Server Process
  ↓
session.event_tx
  ↓
SSE Stream → HTTP Client
```

## Server-Initiated Messages

### Notifications

MCP servers can send notifications to clients at any time using the session's `event_tx`:

```rust
// Example: Server sends progress notification
let event = Event::default()
    .event("notification")
    .data(json!({
        "jsonrpc": "2.0",
        "method": "notifications/progress",
        "params": {
            "progressToken": "abc123",
            "progress": 50,
            "total": 100
        }
    }).to_string());

session.event_tx.send(Ok(event)).await;
```

### Server-Initiated Requests

MCP servers can also initiate requests to clients (e.g., for sampling/createMessage):

```rust
// Example: Server requests LLM completion from client
let event = Event::default()
    .id(event_id.to_string())
    .event("request")
    .data(json!({
        "jsonrpc": "2.0",
        "id": "req_abc123",
        "method": "sampling/createMessage",
        "params": {
            "messages": [...],
            "modelPreferences": {...}
        }
    }).to_string());

session.event_tx.send(Ok(event)).await;
```

## Client Implementation

HTTP clients must:

1. Open SSE stream via GET /mcp to receive bidirectional messages
2. Handle different SSE event types:
   - `data` - Regular JSON-RPC responses
   - `error` - Error responses
   - `notification` - Server-initiated notifications
   - `request` - Server-initiated requests (require client response)

### Example Client Code (JavaScript)

```javascript
// Open SSE stream for bidirectional communication
const eventSource = new EventSource(`http://localhost:3000/mcp`, {
  headers: {
    'Mcp-Session-Id': sessionId
  }
});

// Handle server responses
eventSource.addEventListener('message', (e) => {
  const response = JSON.parse(e.data);
  handleResponse(response);
});

// Handle server notifications
eventSource.addEventListener('notification', (e) => {
  const notification = JSON.parse(e.data);
  handleNotification(notification);
});

// Handle server requests (bidirectional)
eventSource.addEventListener('request', (e) => {
  const request = JSON.parse(e.data);
  
  // Process request and send response back via POST
  const response = await processRequest(request);
  
  await fetch('http://localhost:3000/mcp', {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      'Mcp-Session-Id': sessionId
    },
    body: JSON.stringify(response)
  });
});
```

## Use Cases

### 1. Progress Notifications

Long-running operations can send progress updates:

```json
{
  "jsonrpc": "2.0",
  "method": "notifications/progress",
  "params": {
    "progressToken": "operation_123",
    "progress": 75,
    "total": 100
  }
}
```

### 2. Resource Change Notifications

Servers can notify clients when resources change:

```json
{
  "jsonrpc": "2.0",
  "method": "notifications/resources/list_changed",
  "params": {}
}
```

### 3. LLM Sampling Requests

Servers can request LLM completions from clients:

```json
{
  "jsonrpc": "2.0",
  "id": "req_sampling_001",
  "method": "sampling/createMessage",
  "params": {
    "messages": [{
      "role": "user",
      "content": {
        "type": "text",
        "text": "Generate a summary..."
      }
    }],
    "modelPreferences": {
      "hints": [{"name": "claude-3-5-sonnet-20241022"}]
    },
    "systemPrompt": "You are a helpful assistant.",
    "maxTokens": 1000
  }
}
```

## Implementation Notes

### Event ID Management

Each message sent through SSE gets a unique event ID for resumability:

```rust
let event_id = session.next_event_id();
let event = Event::default()
    .id(event_id.to_string())
    .data(json_data);
```

### Message Buffering

All SSE messages are buffered for replay:

```rust
session.buffer_message(event_id, Some("notification".to_string()), data);
```

This enables clients to resume from Last-Event-ID after disconnection.

### Session Management

- Sessions expire after 1 hour of inactivity (configurable)
- Each session maintains its own message buffer (max 100 messages)
- Session cleanup happens every 60 seconds

## Testing Bidirectional SSE

### Test Server Notification

```bash
# Start MCP Citadel with HTTP transport
mcp-citadel start --foreground --enable-http

# In another terminal, simulate server notification
# (This would typically come from MCP server, shown here for testing)
curl -N -X POST http://127.0.0.1:3000/mcp \
  -H "Content-Type: application/json" \
  -H "Mcp-Session-Id: <session-id>" \
  -H "Mcp-Protocol-Version: 2025-06-18" \
  -d '{
    "jsonrpc": "2.0",
    "method": "notifications/progress",
    "params": {"progress": 50, "total": 100}
  }'
```

### Test Server Request

```bash
# Server initiates sampling request to client
curl -N -X POST http://127.0.0.1:3000/mcp \
  -H "Content-Type: application/json" \
  -H "Mcp-Session-Id: <session-id>" \
  -H "Mcp-Protocol-Version: 2025-06-18" \
  -d '{
    "jsonrpc": "2.0",
    "id": "req_001",
    "method": "sampling/createMessage",
    "params": {
      "messages": [{"role": "user", "content": {"type": "text", "text": "Hello"}}]
    }
  }'
```

## Future Enhancements

- [ ] Automatic retry for failed message delivery
- [ ] Backpressure handling when client is slow
- [ ] Message priority queuing
- [ ] WebSocket transport as alternative to SSE
- [ ] Built-in timeout for server-initiated requests

## Related Documentation

- [HTTP_TRANSPORT.md](HTTP_TRANSPORT.md) - HTTP/SSE transport overview
- [MCP Specification 2025-06-18](https://modelcontextprotocol.io/docs/specification/basic) - Protocol details
