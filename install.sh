#!/bin/bash
# MCP Citadel Installation Script

set -e

echo "🚀 Installing MCP Citadel..."
echo ""

# Build both binaries if not already built
if [ ! -f "target/release/mcp-citadel" ] || [ ! -f "target/release/mcp-client" ]; then
    echo "Building release binaries..."
    cargo build --release --bins
    echo ""
fi

# Copy both binaries to /usr/local/bin
echo "Installing binaries to /usr/local/bin/..."
sudo cp target/release/mcp-citadel /usr/local/bin/
sudo cp target/release/mcp-client /usr/local/bin/
sudo chmod +x /usr/local/bin/mcp-citadel
sudo chmod +x /usr/local/bin/mcp-client

# Verify
echo ""
echo "✅ Installation complete!"
echo ""
echo "Installed binaries:"
echo "  • mcp-citadel    ($(ls -lh /usr/local/bin/mcp-citadel | awk '{print $5}')) - Hub daemon"
echo "  • mcp-client ($(ls -lh /usr/local/bin/mcp-client | awk '{print $5}')) - Client adapter"
echo ""
echo "Quick start:"
echo "  1. List servers:    mcp-citadel servers"
echo "  2. Start hub:       mcp-citadel start"
echo "  3. Check status:    mcp-citadel status"
echo ""
echo "Next: Update Claude config to use mcp-client (see PRODUCTION.md)"
echo ""
