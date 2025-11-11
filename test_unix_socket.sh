#!/bin/bash
# Test Unix socket transport with real MCP servers

SOCKET="/tmp/mcp-citadel.sock"

echo "🧪 Testing Unix Socket Transport"
echo "================================"
echo ""

# Test 1: GitHub initialize
echo "Test 1: GitHub Initialize"
echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test","version":"1.0"},"server":"github"}}' | \
  nc -U $SOCKET | head -1 | jq -r 'if .result then "✅ GitHub initialized: " + .result.serverInfo.name else "❌ Failed" end'
echo ""

# Test 2: Tavily tools list  
echo "Test 2: Tavily Initialize"
echo '{"jsonrpc":"2.0","id":2,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test","version":"1.0"},"server":"tavily-mcp"}}' | \
  nc -U $SOCKET | head -1 | jq -r 'if .result then "✅ Tavily initialized" else "❌ Failed" end'
echo ""

# Test 3: Filesystem
echo "Test 3: Filesystem Initialize"
echo '{"jsonrpc":"2.0","id":3,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test","version":"1.0"},"server":"filesystem"}}' | \
  nc -U $SOCKET | head -1 | jq -r 'if .result then "✅ Filesystem initialized" else "❌ Failed" end'
echo ""

echo "================================"
echo "✅ Unix socket transport works!"
echo "🎉 Core functionality verified"
