#!/usr/bin/env python3
"""Integration tests with real MCP servers via HTTP transport"""

import requests
import json
import time
from datetime import datetime

BASE_URL = "http://127.0.0.1:3000/mcp"
HEADERS = {
    "Content-Type": "application/json",
    "MCP-Protocol-Version": "2025-06-18"
}

def test_github_initialize():
    """Test GitHub server initialization"""
    print("🧪 Test: GitHub Initialize")
    payload = {
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2025-06-18",
            "capabilities": {},
            "clientInfo": {
                "name": "integration-test",
                "version": "1.0.0"
            },
            "server": "github"
        }
    }
    
    try:
        print("   Sending initialize to GitHub server...")
        response = requests.post(
            BASE_URL,
            headers=HEADERS,
            json=payload,
            timeout=15
        )
        
        print(f"   Status: {response.status_code}")
        
        if response.status_code == 200:
            session_id = response.headers.get("Mcp-Session-Id")
            if session_id:
                print(f"   ✅ Session created: {session_id[:20]}...")
            
            data = response.json()
            if "result" in data:
                result = data["result"]
                print(f"   ✅ Server info: {result.get('serverInfo', {}).get('name', 'Unknown')}")
                if "capabilities" in result:
                    print(f"   ✅ Capabilities: {list(result['capabilities'].keys())}")
                return session_id
            else:
                print(f"   ⚠️  Response: {json.dumps(data, indent=2)}")
        else:
            print(f"   ❌ Failed with status {response.status_code}")
            print(f"   Response: {response.text}")
            
    except requests.Timeout:
        print("   ⚠️  Request timed out")
    except Exception as e:
        print(f"   ❌ Error: {e}")
    
    return None

def test_github_tools_list(session_id):
    """Test listing GitHub tools"""
    print("\n🧪 Test: GitHub Tools List")
    
    if not session_id:
        print("   ⚠️  Skipped (no session)")
        return False
    
    payload = {
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/list",
        "params": {
            "server": "github"
        }
    }
    
    headers = HEADERS.copy()
    headers["Mcp-Session-Id"] = session_id
    
    try:
        print("   Requesting tools list...")
        response = requests.post(
            BASE_URL,
            headers=headers,
            json=payload,
            timeout=10
        )
        
        print(f"   Status: {response.status_code}")
        
        if response.status_code == 200:
            data = response.json()
            if "result" in data and "tools" in data["result"]:
                tools = data["result"]["tools"]
                print(f"   ✅ Got {len(tools)} tools")
                for tool in tools[:3]:  # Show first 3
                    print(f"      • {tool.get('name', 'Unknown')}")
                if len(tools) > 3:
                    print(f"      ... and {len(tools) - 3} more")
                return True
            else:
                print(f"   ⚠️  Unexpected response: {json.dumps(data, indent=2)[:200]}")
        else:
            print(f"   ❌ Failed: {response.text}")
            
    except Exception as e:
        print(f"   ❌ Error: {e}")
    
    return False

def test_filesystem_resources():
    """Test filesystem server resources"""
    print("\n🧪 Test: Filesystem Resources")
    
    # First initialize
    payload = {
        "jsonrpc": "2.0",
        "id": 10,
        "method": "initialize",
        "params": {
            "protocolVersion": "2025-06-18",
            "capabilities": {},
            "clientInfo": {"name": "test", "version": "1.0"},
            "server": "filesystem"
        }
    }
    
    try:
        print("   Initializing filesystem server...")
        response = requests.post(BASE_URL, headers=HEADERS, json=payload, timeout=10)
        
        if response.status_code == 200:
            session_id = response.headers.get("Mcp-Session-Id")
            print(f"   ✅ Filesystem initialized")
            
            # Now list resources
            payload = {
                "jsonrpc": "2.0",
                "id": 11,
                "method": "resources/list",
                "params": {"server": "filesystem"}
            }
            
            headers = HEADERS.copy()
            headers["Mcp-Session-Id"] = session_id
            
            response = requests.post(BASE_URL, headers=headers, json=payload, timeout=10)
            
            if response.status_code == 200:
                data = response.json()
                if "result" in data:
                    print(f"   ✅ Got resources response")
                    return True
            else:
                print(f"   ⚠️  Status: {response.status_code}")
        else:
            print(f"   ❌ Init failed: {response.status_code}")
            
    except Exception as e:
        print(f"   ❌ Error: {e}")
    
    return False

def test_tavily_tools():
    """Test Tavily MCP server"""
    print("\n🧪 Test: Tavily Tools")
    
    payload = {
        "jsonrpc": "2.0",
        "id": 20,
        "method": "initialize",
        "params": {
            "protocolVersion": "2025-06-18",
            "capabilities": {},
            "clientInfo": {"name": "test", "version": "1.0"},
            "server": "tavily-mcp"
        }
    }
    
    try:
        response = requests.post(BASE_URL, headers=HEADERS, json=payload, timeout=10)
        
        if response.status_code == 200:
            session_id = response.headers.get("Mcp-Session-Id")
            print(f"   ✅ Tavily initialized")
            
            # List tools
            payload = {
                "jsonrpc": "2.0",
                "id": 21,
                "method": "tools/list",
                "params": {"server": "tavily-mcp"}
            }
            
            headers = HEADERS.copy()
            headers["Mcp-Session-Id"] = session_id
            
            response = requests.post(BASE_URL, headers=headers, json=payload, timeout=10)
            
            if response.status_code == 200:
                data = response.json()
                if "result" in data and "tools" in data["result"]:
                    tools = data["result"]["tools"]
                    print(f"   ✅ Got {len(tools)} Tavily tools")
                    for tool in tools:
                        print(f"      • {tool.get('name', 'Unknown')}")
                    return True
        else:
            print(f"   ⚠️  Status: {response.status_code}")
            
    except Exception as e:
        print(f"   ❌ Error: {e}")
    
    return False

def main():
    print("=" * 60)
    print("MCP Citadel HTTP Transport - Integration Tests")
    print("=" * 60)
    print(f"Time: {datetime.now()}")
    print(f"URL: {BASE_URL}")
    print()
    
    results = []
    
    # Test GitHub
    print("=" * 60)
    print("Testing GitHub MCP Server")
    print("=" * 60)
    session_id = test_github_initialize()
    results.append(("GitHub Initialize", session_id is not None))
    
    if session_id:
        time.sleep(1)  # Brief pause
        tools_ok = test_github_tools_list(session_id)
        results.append(("GitHub Tools List", tools_ok))
    
    # Test Filesystem
    print("\n" + "=" * 60)
    print("Testing Filesystem MCP Server")
    print("=" * 60)
    fs_ok = test_filesystem_resources()
    results.append(("Filesystem Resources", fs_ok))
    
    # Test Tavily
    print("\n" + "=" * 60)
    print("Testing Tavily MCP Server")
    print("=" * 60)
    tavily_ok = test_tavily_tools()
    results.append(("Tavily Tools", tavily_ok))
    
    # Summary
    print("\n" + "=" * 60)
    print("Summary")
    print("=" * 60)
    passed = sum(1 for _, result in results if result)
    total = len(results)
    print(f"Passed: {passed}/{total}")
    print()
    
    for name, result in results:
        status = "✅" if result else "❌"
        print(f"  {status} {name}")
    
    if passed == total:
        print("\n🎉 All integration tests passed!")
        print("✨ HTTP transport is fully functional with real MCP servers!")
    else:
        print(f"\n⚠️  {total - passed} test(s) failed")

if __name__ == "__main__":
    main()
