#!/usr/bin/env python3
"""
Test client for CrabCache Docker container
"""

import socket
import time

def test_crabcache():
    host = 'localhost'
    port = 7004  # Docker mapped port
    
    try:
        print(f"Connecting to CrabCache at {host}:{port}...")
        sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        sock.settimeout(5)
        sock.connect((host, port))
        print("Connected!")
        
        # Test PING
        print("Sending PING...")
        sock.send(b"PING\r\n")
        response = sock.recv(1024).decode().strip()
        print(f"Response: {response}")
        
        if response == "PONG":
            print("✅ PING test passed!")
        else:
            print(f"❌ PING test failed. Expected 'PONG', got '{response}'")
            return False
        
        # Test PUT
        print("Sending PUT docker_key docker_value...")
        sock.send(b"PUT docker_key docker_value\r\n")
        response = sock.recv(1024).decode().strip()
        print(f"Response: {response}")
        
        # Test GET
        print("Sending GET docker_key...")
        sock.send(b"GET docker_key\r\n")
        response = sock.recv(1024).decode().strip()
        print(f"Response: {response}")
        
        if response == "docker_value":
            print("✅ PUT/GET test passed!")
        else:
            print(f"❌ PUT/GET test failed. Expected 'docker_value', got '{response}'")
            return False
        
        # Test STATS
        print("Sending STATS...")
        sock.send(b"STATS\r\n")
        response = sock.recv(1024).decode().strip()
        print(f"Response: {response}")
        
        sock.close()
        print("✅ All tests passed! CrabCache Docker container is working!")
        return True
        
    except ConnectionRefusedError:
        print(f"❌ Could not connect to {host}:{port}")
        print("Make sure the CrabCache Docker container is running:")
        print("docker run -d -p 7002:7000 crabcache:latest")
        return False
    except Exception as e:
        print(f"❌ Error: {e}")
        return False

if __name__ == "__main__":
    test_crabcache()