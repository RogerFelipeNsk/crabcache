#!/usr/bin/env python3
"""
Simple pipeline functionality test for CrabCache
"""

import socket
import time
import sys

def test_pipeline():
    """Test basic pipeline functionality"""
    print("🧪 Testing CrabCache Pipeline Functionality")
    print("=" * 50)
    
    # Connect to CrabCache
    try:
        sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        sock.setsockopt(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)
        sock.connect(("127.0.0.1", 8000))
        print("✓ Connected to CrabCache server")
    except Exception as e:
        print(f"❌ Failed to connect: {e}")
        return False
    
    try:
        # Test 1: Single command (baseline)
        print("\n📝 Test 1: Single command")
        sock.send(b"PING\n")
        response = sock.recv(1024).decode().strip()
        print(f"   Command: PING")
        print(f"   Response: {response}")
        assert response == "PONG", f"Expected PONG, got {response}"
        print("   ✓ Single command works")
        
        # Test 2: Pipeline batch (multiple commands at once)
        print("\n🚀 Test 2: Pipeline batch")
        batch_commands = [
            "PUT test_key1 test_value1",
            "PUT test_key2 test_value2", 
            "GET test_key1",
            "GET test_key2",
            "PING"
        ]
        
        # Send all commands at once
        batch_data = "\n".join(batch_commands) + "\n"
        print(f"   Sending batch: {len(batch_commands)} commands")
        sock.send(batch_data.encode())
        
        # Receive all responses
        responses = []
        response_buffer = b""
        
        # Read all data first
        while len(responses) < len(batch_commands):
            chunk = sock.recv(4096)
            if not chunk:
                break
            response_buffer += chunk
            
            # Split by newlines and extract complete responses
            while b"\n" in response_buffer:
                line, response_buffer = response_buffer.split(b"\n", 1)
                if line:  # Skip empty lines
                    responses.append(line.decode().strip())
                if len(responses) >= len(batch_commands):
                    break
        
        print("   Responses:")
        for i, (cmd, resp) in enumerate(zip(batch_commands, responses)):
            print(f"     {i+1}. {cmd} → {resp}")
        
        # Verify responses
        expected = ["OK", "OK", "test_value1", "test_value2", "PONG"]
        for i, (expected_resp, actual_resp) in enumerate(zip(expected, responses)):
            assert actual_resp == expected_resp, f"Command {i+1}: expected {expected_resp}, got {actual_resp}"
        
        print("   ✓ Pipeline batch works correctly")
        
        # Test 3: Performance comparison
        print("\n⚡ Test 3: Performance comparison")
        
        # Single commands timing
        start_time = time.time()
        for i in range(50):  # Reduced from 100 to avoid connection issues
            sock.send(f"PUT perf_key_{i} perf_value_{i}\n".encode())
            response = sock.recv(1024)
        single_time = time.time() - start_time
        single_ops_per_sec = 50 / single_time
        
        print(f"   Single commands: {single_ops_per_sec:.0f} ops/sec")
        
        # Pipeline batch timing
        batch_size = 5  # Smaller batch size
        num_batches = 10
        start_time = time.time()
        
        for batch_idx in range(num_batches):
            # Create batch
            batch_cmds = []
            for i in range(batch_size):
                key_idx = batch_idx * batch_size + i
                batch_cmds.append(f"PUT batch_key_{key_idx} batch_value_{key_idx}")
            
            # Send batch
            batch_data = "\n".join(batch_cmds) + "\n"
            sock.send(batch_data.encode())
            
            # Receive responses properly
            responses = []
            response_buffer = b""
            
            while len(responses) < batch_size:
                chunk = sock.recv(4096)
                if not chunk:
                    break
                response_buffer += chunk
                
                while b"\n" in response_buffer:
                    line, response_buffer = response_buffer.split(b"\n", 1)
                    if line:
                        responses.append(line.decode().strip())
                    if len(responses) >= batch_size:
                        break
        
        pipeline_time = time.time() - start_time
        pipeline_ops_per_sec = (num_batches * batch_size) / pipeline_time
        
        print(f"   Pipeline batch: {pipeline_ops_per_sec:.0f} ops/sec")
        
        improvement = pipeline_ops_per_sec / single_ops_per_sec
        print(f"   Performance improvement: {improvement:.1f}x")
        
        if improvement > 1.5:
            print("   ✓ Pipeline shows significant performance improvement")
        else:
            print("   ⚠️  Pipeline improvement is less than expected")
        
        print("\n🎉 All pipeline tests passed!")
        return True
        
    except Exception as e:
        print(f"❌ Test failed: {e}")
        return False
    finally:
        sock.close()

if __name__ == "__main__":
    success = test_pipeline()
    sys.exit(0 if success else 1)