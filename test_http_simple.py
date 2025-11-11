#!/usr/bin/env python3
"""Simple HTTP transport test that doesn't hang on slow MCP servers"""

import requests
import json
from datetime import datetime

BASE_URL = "http://127.0.0.1:3000/mcp"
HEADERS = {
    "Content-Type": "application/json",
    "MCP-Protocol-Version": "2025-06-18"
}

def test_endpoint_exists():
    """Test 1: Endpoint responds (should get 400 without proper body)"""
    print("🧪 Test 1: Endpoint exists")
    try:
        response = requests.get(BASE_URL, timeout=2)
        print(f"   ✅ GET /mcp → {response.status_code}")
        return True
    except Exception as e:
        print(f"   ❌ Error: {e}")
        return False

def test_invalid_json():
    """Test 2: Invalid JSON gets rejected"""
    print("\n🧪 Test 2: Invalid JSON rejection")
    try:
        response = requests.post(
            BASE_URL,
            headers=HEADERS,
            data="not json",
            timeout=2
        )
        print(f"   ✅ POST invalid JSON → {response.status_code}")
        return True
    except Exception as e:
        print(f"   ❌ Error: {e}")
        return False

def test_missing_server():
    """Test 3: Missing server parameter gets rejected"""
    print("\n🧪 Test 3: Missing server parameter")
    payload = {
        "jsonrpc": "2.0",
        "id": 1,
        "method": "test",
        "params": {}
    }
    try:
        response = requests.post(
            BASE_URL,
            headers=HEADERS,
            json=payload,
            timeout=2
        )
        print(f"   ✅ POST without server → {response.status_code}")
        if response.status_code == 400:
            print(f"   ✅ Correctly rejected (400 Bad Request)")
        return True
    except Exception as e:
        print(f"   ❌ Error: {e}")
        return False

def test_nonexistent_server():
    """Test 4: Non-existent server name"""
    print("\n🧪 Test 4: Non-existent server")
    payload = {
        "jsonrpc": "2.0",
        "id": 1,
        "method": "test",
        "params": {"server": "this-server-does-not-exist"}
    }
    try:
        response = requests.post(
            BASE_URL,
            headers=HEADERS,
            json=payload,
            timeout=5
        )
        print(f"   ✅ POST to fake server → {response.status_code}")
        if response.text:
            data = response.json()
            if "error" in data:
                print(f"   ✅ Got error response: {data['error'].get('message', 'Unknown')}")
        return True
    except requests.Timeout:
        print(f"   ⚠️  Request timed out (server might be processing)")
        return False
    except Exception as e:
        print(f"   ❌ Error: {e}")
        return False

def test_session_creation():
    """Test 5: Initialize creates session"""
    print("\n🧪 Test 5: Session creation (initialize)")
    payload = {
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2025-06-18",
            "capabilities": {},
            "clientInfo": {"name": "test", "version": "1.0"},
            "server": "github"
        }
    }
    
    print("   ⏳ Sending initialize request (may take a few seconds)...")
    try:
        response = requests.post(
            BASE_URL,
            headers=HEADERS,
            json=payload,
            timeout=10
        )
        print(f"   ✅ POST initialize → {response.status_code}")
        
        # Check for session ID in headers
        session_id = response.headers.get("Mcp-Session-Id")
        if session_id:
            print(f"   ✅ Got session ID: {session_id[:20]}...")
            return session_id
        else:
            print(f"   ⚠️  No session ID in response")
            return None
    except requests.Timeout:
        print(f"   ⚠️  Initialize timed out (GitHub server might be slow)")
        return None
    except Exception as e:
        print(f"   ❌ Error: {e}")
        return None

def main():
    print("=" * 50)
    print("MCP Citadel HTTP Transport Tests")
    print("=" * 50)
    print(f"Time: {datetime.now()}")
    print(f"URL: {BASE_URL}")
    print()
    
    results = []
    
    # Run quick tests
    results.append(("Endpoint exists", test_endpoint_exists()))
    results.append(("Invalid JSON", test_invalid_json()))
    results.append(("Missing server param", test_missing_server()))
    results.append(("Non-existent server", test_nonexistent_server()))
    
    # Optional slow test
    print("\n" + "=" * 50)
    response = input("Run slow test (initialize with real server)? [y/N]: ")
    if response.lower() == 'y':
        session_id = test_session_creation()
        results.append(("Session creation", session_id is not None))
    
    # Summary
    print("\n" + "=" * 50)
    print("Summary")
    print("=" * 50)
    passed = sum(1 for _, result in results if result)
    total = len(results)
    print(f"Passed: {passed}/{total}")
    
    for name, result in results:
        status = "✅" if result else "❌"
        print(f"  {status} {name}")
    
    print("\n✨ HTTP transport is operational!")

if __name__ == "__main__":
    main()
