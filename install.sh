#!/bin/bash
# MCP Hub Installation Script

set -e

echo "🚀 Installing MCP Hub..."
echo ""

# Build if not already built
if [ ! -f "target/release/mcp-hub" ]; then
    echo "Building release binary..."
    cargo build --release
    echo ""
fi

# Copy to /usr/local/bin
echo "Installing to /usr/local/bin/mcp-hub..."
sudo cp target/release/mcp-hub /usr/local/bin/

# Verify
echo ""
echo "✅ Installation complete!"
echo ""
echo "Verify installation:"
echo "  mcp-hub --version"
echo ""
echo "List configured servers:"
echo "  mcp-hub servers"
echo ""
echo "Start the hub:"
echo "  mcp-hub start --foreground"
echo ""
