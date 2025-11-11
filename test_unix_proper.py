#!/usr/bin/env python3
"""Test Unix socket with proper MCP handshake"""

import socket
import json

SOCKET_PATH = "/tmp/mcp-citadel.sock"

def send_message(sock, message):
    """Send JSON-RPC message with newline"""
    data = json.dumps(message) + "\n"
    sock.sendall(data.encode('utf-8'))

def receive_message(sock):
    """Receive one JSON-RPC message (newline-delimited)"""
    buffer = b""
    while True:
        chunk = sock.recv(1024)
        if not chunk:
            break
        buffer += chunk
        if b'\n' in buffer:
            line, _ = buffer.split(b'\n', 1)
            return json.loads(line.decode('utf-8'))
    return None

def test_github():
    """Test GitHub server with proper handshake"""
    print("🧪 Testing GitHub MCP Server")
    
    sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    sock.connect(SOCKET_PATH)
    sock.settimeout(5.0)
    
    try:
        # Step 1: Send initialize
        print("   1. Sending initialize...")
        send_message(sock, {
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": {"name": "test", "version": "1.0"},
                "server": "github"
            }
        })
        
        # Step 2: Wait for InitializeResult
        print("   2. Waiting for InitializeResult...")
        response = receive_message(sock)
        
        if response and "result" in response:
            server_name = response["result"].get("serverInfo", {}).get("name", "Unknown")
            print(f"   ✅ Got InitializeResult: {server_name}")
            
            # Step 3: Send initialized notification (CRITICAL!)
            print("   3. Sending initialized notification...")
            send_message(sock, {
                "jsonrpc": "2.0",
                "method": "notifications/initialized"
            })
            
            print("   ✅ Handshake complete!")
            
            # Now we can send actual requests
            print("   4. Requesting tools list...")
            send_message(sock, {
                "jsonrpc": "2.0",
                "id": 2,
                "method": "tools/list",
                "params": {"server": "github"}
            })
            
            tools_response = receive_message(sock)
            if tools_response and "result" in tools_response:
                tools = tools_response["result"].get("tools", [])
                print(f"   ✅ Got {len(tools)} tools!")
                for tool in tools[:3]:
                    print(f"      • {tool['name']}")
                return True
        else:
            print(f"   ❌ Unexpected response: {response}")
            
    except socket.timeout:
        print("   ⏱️  Timeout waiting for response")
    except Exception as e:
        print(f"   ❌ Error: {e}")
    finally:
        sock.close()
    
    return False

def main():
    print("=" * 50)
    print("MCP Unix Socket - Proper Protocol Test")
    print("=" * 50)
    print()
    
    success = test_github()
    
    print()
    print("=" * 50)
    if success:
        print("✅ TEST PASSED")
        print("🎉 Unix socket transport works perfectly!")
    else:
        print("❌ TEST FAILED")
    print("=" * 50)

if __name__ == "__main__":
    main()
