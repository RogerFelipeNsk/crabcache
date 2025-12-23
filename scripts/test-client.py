#!/usr/bin/env python3
"""
Simple test client for CrabCache server
"""

import socket
import time
import sys

def send_command(sock, command):
    """Send a command and receive response"""
    print(f"Sending: {command.strip()}")
    sock.send(command.encode())
    response = sock.recv(1024).decode().strip()
    print(f"Response: {response}")
    return response

def main():
    host = '127.0.0.1'
    port = 7000
    
    try:
        # Connect to server
        print(f"Connecting to CrabCache server at {host}:{port}...")
        sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        sock.connect((host, port))
        print("Connected!")
        
        # Test commands
        commands = [
            "PING\r\n",
            "PUT hello world\r\n",
            "GET hello\r\n",
            "PUT test_key test_value 3600\r\n",
            "GET test_key\r\n",
            "EXPIRE hello 1800\r\n",
            "STATS\r\n",
            "DEL hello\r\n",
            "GET hello\r\n",
        ]
        
        for cmd in commands:
            send_command(sock, cmd)
            time.sleep(0.1)  # Small delay between commands
        
        print("\nTest completed successfully!")
        
    except ConnectionRefusedError:
        print(f"Error: Could not connect to server at {host}:{port}")
        print("Make sure the CrabCache server is running with: cargo run")
        sys.exit(1)
    except Exception as e:
        print(f"Error: {e}")
        sys.exit(1)
    finally:
        if 'sock' in locals():
            sock.close()

if __name__ == "__main__":
    main()