#!/bin/bash
# Install MCP Citadel as macOS LaunchAgent

set -e

PLIST_SOURCE="$(pwd)/com.mcp-citadel.plist"
PLIST_DEST="$HOME/Library/LaunchAgents/com.mcp-citadel.plist"

echo "🚀 Installing MCP Citadel LaunchAgent..."
echo ""

# Create LaunchAgents directory if it doesn't exist
mkdir -p "$HOME/Library/LaunchAgents"

# Create .mcp-citadel directory for logs
mkdir -p "$HOME/.mcp-citadel"

# Copy plist
echo "Installing plist to $PLIST_DEST..."
cp "$PLIST_SOURCE" "$PLIST_DEST"

# Load the service
echo "Loading service..."
launchctl load "$PLIST_DEST"

# Verify
echo ""
echo "✅ MCP Citadel service installed!"
echo ""
echo "Service status:"
launchctl list | grep mcp-citadel || echo "  (Service will start on next login or run: launchctl start com.mcp-citadel)"

echo ""
echo "Log files:"
echo "  • Hub log:   ~/.mcp-citadel/hub.log"
echo "  • Stdout:    ~/.mcp-citadel/stdout.log"
echo "  • Stderr:    ~/.mcp-citadel/stderr.log"
echo "  • Status:    ~/.mcp-citadel/status.json"
echo ""
echo "Commands:"
echo "  • Start:     launchctl start com.mcp-citadel"
echo "  • Stop:      launchctl stop com.mcp-citadel"
echo "  • Restart:   launchctl kickstart -k gui/\$(id -u)/com.mcp-citadel"
echo "  • Uninstall: ./uninstall-service.sh"
echo ""
