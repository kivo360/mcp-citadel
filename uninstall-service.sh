#!/bin/bash
# Uninstall MCP Citadel LaunchAgent

set -e

PLIST_DEST="$HOME/Library/LaunchAgents/com.mcp-citadel.plist"

echo "🗑️  Uninstalling MCP Citadel LaunchAgent..."
echo ""

# Stop and unload the service
if [ -f "$PLIST_DEST" ]; then
    echo "Stopping service..."
    launchctl stop com.mcp-citadel 2>/dev/null || true
    
    echo "Unloading service..."
    launchctl unload "$PLIST_DEST" 2>/dev/null || true
    
    echo "Removing plist..."
    rm "$PLIST_DEST"
    
    echo ""
    echo "✅ MCP Citadel service uninstalled!"
else
    echo "⚠️  Service not found (already uninstalled?)"
fi

echo ""
echo "Note: Log files in ~/.mcp-citadel/ were not removed"
echo "Run: rm -rf ~/.mcp-citadel to remove them"
echo ""
