#!/bin/bash
# Quick HTTP transport test script

set -e

echo "🧪 MCP Citadel HTTP Transport Test"
echo "=================================="
echo ""

# Check if server is running
if ! curl -s -o /dev/null http://127.0.0.1:3000/mcp; then
    echo "❌ Error: HTTP server not running"
    echo ""
    echo "Start the server first:"
    echo "  mcp-citadel start --foreground --enable-http"
    exit 1
fi

echo "✅ Server is running"
echo ""

# Test 1: Initialize request
echo "📤 Test 1: Initialize Request"
RESPONSE=$(curl -s -X POST http://127.0.0.1:3000/mcp \
  -H "Content-Type: application/json" \
  -H "MCP-Protocol-Version: 2025-06-18" \
  -H "Accept: application/json" \
  -i \
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
      },
      "server": "github"
    }
  }')

echo "$RESPONSE"
echo ""

# Extract session ID
SESSION_ID=$(echo "$RESPONSE" | grep -i "mcp-session-id:" | cut -d' ' -f2 | tr -d '\r')

if [ -z "$SESSION_ID" ]; then
    echo "❌ Error: No session ID returned"
    exit 1
fi

echo "✅ Got session ID: $SESSION_ID"
echo ""

# Test 2: Tools list with session
echo "📤 Test 2: Tools List Request"
curl -s -X POST http://127.0.0.1:3000/mcp \
  -H "Content-Type: application/json" \
  -H "MCP-Protocol-Version: 2025-06-18" \
  -H "Mcp-Session-Id: $SESSION_ID" \
  -H "Accept: application/json" \
  -d "{
    \"jsonrpc\": \"2.0\",
    \"id\": 2,
    \"method\": \"tools/list\",
    \"params\": {
      \"server\": \"github\"
    }
  }" | jq '.'

echo ""
echo "✅ All tests passed!"
echo ""
echo "Next steps:"
echo "  - Try SSE stream: curl -N -H 'Mcp-Session-Id: $SESSION_ID' http://127.0.0.1:3000/mcp"
echo "  - See HTTP_TRANSPORT.md for full documentation"
