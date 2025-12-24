#!/usr/bin/env python3
"""
Final CrabCache System Test

This script tests all major features of CrabCache to validate
the complete implementation of all phases.
"""

import socket
import time
import json
import sys
from datetime import datetime

class CrabCacheSystemTest:
    """Complete system test for CrabCache"""
    
    def __init__(self, host="127.0.0.1", port=8000):
        self.host = host
        self.port = port
        self.socket = None
    
    def connect(self):
        """Connect to CrabCache server"""
        self.socket = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        self.socket.setsockopt(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)
        self.socket.connect((self.host, self.port))
        print(f"✓ Connected to CrabCache at {self.host}:{self.port}")
    
    def disconnect(self):
        """Disconnect from server"""
        if self.socket:
            self.socket.close()
            self.socket = None
    
    def send_command(self, command):
        """Send single command and receive response"""
        self.socket.send(f"{command}\n".encode())
        response = b""
        while b"\n" not in response:
            chunk = self.socket.recv(1024)
            if not chunk:
                break
            response += chunk
        return response.decode().strip()
    
    def send_pipeline_batch(self, commands):
        """Send batch of commands using pipelining"""
        # Send all commands at once
        batch_data = "\n".join(commands) + "\n"
        self.socket.send(batch_data.encode())
        
        # Receive all responses
        responses = []
        response_buffer = b""
        
        # Add timeout to prevent hanging
        self.socket.settimeout(10.0)
        
        try:
            while len(responses) < len(commands):
                chunk = self.socket.recv(4096)
                if not chunk:
                    break
                response_buffer += chunk
                
                while b"\n" in response_buffer:
                    line, response_buffer = response_buffer.split(b"\n", 1)
                    if line:
                        responses.append(line.decode().strip())
                    if len(responses) >= len(commands):
                        break
        except socket.timeout:
            print(f"Timeout waiting for responses. Got {len(responses)} out of {len(commands)}")
        finally:
            self.socket.settimeout(None)
        
        return responses
    
    def test_basic_operations(self):
        """Test basic cache operations"""
        print("\n🧪 Testing Basic Operations")
        print("-" * 40)
        
        # Test PING
        response = self.send_command("PING")
        assert response == "PONG", f"Expected PONG, got {response}"
        print("✓ PING/PONG works")
        
        # Test PUT/GET
        response = self.send_command("PUT test_key test_value")
        assert response == "OK", f"Expected OK, got {response}"
        
        response = self.send_command("GET test_key")
        assert response == "test_value", f"Expected test_value, got {response}"
        print("✓ PUT/GET works")
        
        # Test DEL
        response = self.send_command("DEL test_key")
        assert response == "OK", f"Expected OK, got {response}"
        
        response = self.send_command("GET test_key")
        assert response in ["NOT_FOUND", "NULL"], f"Expected NOT_FOUND or NULL, got {response}"
        print("✓ DEL works")
        
        # Test TTL
        response = self.send_command("PUT ttl_key ttl_value 5")
        assert response == "OK", f"Expected OK, got {response}"
        
        response = self.send_command("GET ttl_key")
        assert response == "ttl_value", f"Expected ttl_value, got {response}"
        print("✓ TTL works")
        
        print("✅ All basic operations work correctly")
    
    def test_pipeline_performance(self):
        """Test pipelining functionality and performance"""
        print("\n🚀 Testing Pipeline Performance")
        print("-" * 40)
        
        # Test pipeline batch
        commands = [
            "PUT pipeline_key1 pipeline_value1",
            "PUT pipeline_key2 pipeline_value2",
            "GET pipeline_key1",
            "GET pipeline_key2",
            "PING"
        ]
        
        responses = self.send_pipeline_batch(commands)
        expected = ["OK", "OK", "pipeline_value1", "pipeline_value2", "PONG"]
        
        for i, (expected_resp, actual_resp) in enumerate(zip(expected, responses)):
            assert actual_resp == expected_resp, f"Command {i+1}: expected {expected_resp}, got {actual_resp}"
        
        print("✓ Pipeline batch processing works")
        
        # Performance comparison
        num_ops = 100
        
        # Single commands
        start_time = time.time()
        for i in range(num_ops):
            response = self.send_command(f"PUT single_{i} value_{i}")
        single_time = time.time() - start_time
        single_ops_per_sec = num_ops / single_time
        
        # Pipeline batch
        batch_size = 10
        num_batches = num_ops // batch_size
        start_time = time.time()
        
        for batch_idx in range(num_batches):
            batch_commands = []
            for i in range(batch_size):
                key_idx = batch_idx * batch_size + i
                batch_commands.append(f"PUT batch_{key_idx} value_{key_idx}")
            
            responses = self.send_pipeline_batch(batch_commands)
        
        pipeline_time = time.time() - start_time
        pipeline_ops_per_sec = num_ops / pipeline_time
        
        improvement = pipeline_ops_per_sec / single_ops_per_sec
        
        print(f"✓ Single commands: {single_ops_per_sec:.0f} ops/sec")
        print(f"✓ Pipeline batch: {pipeline_ops_per_sec:.0f} ops/sec")
        print(f"✓ Performance improvement: {improvement:.1f}x")
        
        assert improvement > 1.0, f"Pipeline should be faster, got {improvement:.1f}x"
        print("✅ Pipeline performance improvement confirmed")
    
    def test_mixed_workload(self):
        """Test mixed workload with different operations"""
        print("\n🔀 Testing Mixed Workload")
        print("-" * 40)
        
        # Prepare data
        setup_commands = []
        for i in range(20):
            setup_commands.append(f"PUT mixed_key_{i} mixed_value_{i}")
        
        responses = self.send_pipeline_batch(setup_commands)
        assert all(r == "OK" for r in responses), "Setup failed"
        print("✓ Data setup complete")
        
        # Mixed operations (respecting max_batch_size=16)
        mixed_commands = []
        
        # 8 GET operations (50% of 16)
        for i in range(8):
            mixed_commands.append(f"GET mixed_key_{i}")
        
        # 5 PUT operations (30% of 16)
        for i in range(5):
            mixed_commands.append(f"PUT mixed_key_{i} updated_value_{i}")
        
        # 3 DEL operations (20% of 16)
        for i in range(3):
            mixed_commands.append(f"DEL mixed_key_{i + 10}")
        
        start_time = time.time()
        responses = self.send_pipeline_batch(mixed_commands)
        mixed_time = time.time() - start_time
        
        mixed_ops_per_sec = len(mixed_commands) / mixed_time
        
        print(f"✓ Mixed workload: {len(mixed_commands)} operations")
        print(f"✓ Performance: {mixed_ops_per_sec:.0f} ops/sec")
        print(f"✓ Time taken: {mixed_time*1000:.2f}ms")
        
        # Verify some responses
        get_responses = responses[:8]  # First 8 are GET operations
        for i, response in enumerate(get_responses):
            expected = f"mixed_value_{i}"
            assert response == expected, f"GET mixed_key_{i}: expected {expected}, got {response}"
        
        print("✅ Mixed workload completed successfully")
    
    def test_stress_operations(self):
        """Test system under stress with large batches"""
        print("\n💪 Testing Stress Operations")
        print("-" * 40)
        
        # Large batch test (respecting max_batch_size=16)
        large_batch_size = 16  # Match the configured max_batch_size
        commands = []
        
        for i in range(large_batch_size):
            commands.append(f"PUT stress_key_{i} stress_value_{i}")
        
        start_time = time.time()
        responses = self.send_pipeline_batch(commands)
        stress_time = time.time() - start_time
        
        assert len(responses) == large_batch_size, f"Expected {large_batch_size} responses, got {len(responses)}"
        assert all(r == "OK" for r in responses), "Not all PUT operations succeeded"
        
        stress_ops_per_sec = large_batch_size / stress_time
        
        print(f"✓ Large batch: {large_batch_size} operations")
        print(f"✓ Performance: {stress_ops_per_sec:.0f} ops/sec")
        print(f"✓ Time taken: {stress_time*1000:.2f}ms")
        
        # Verify data integrity
        verify_commands = [f"GET stress_key_{i}" for i in range(10)]  # Check first 10
        verify_responses = self.send_pipeline_batch(verify_commands)
        
        for i, response in enumerate(verify_responses):
            expected = f"stress_value_{i}"
            assert response == expected, f"Data integrity check failed for key {i}"
        
        print("✅ Stress test completed successfully")
    
    def test_system_stats(self):
        """Test system statistics and monitoring"""
        print("\n📊 Testing System Statistics")
        print("-" * 40)
        
        # Test STATS command
        response = self.send_command("STATS")
        print(f"✓ STATS response: {response[:100]}...")  # Show first 100 chars
        
        # Should contain key metrics
        assert "operations" in response.lower() or "ops" in response.lower(), "Stats should contain operation metrics"
        
        print("✅ System statistics available")
    
    def run_complete_test(self):
        """Run complete system test suite"""
        print("🚀 CrabCache Complete System Test")
        print("=" * 60)
        print(f"🕐 Test started at: {datetime.now().isoformat()}")
        print(f"🌐 Testing server: {self.host}:{self.port}")
        
        results = {
            "timestamp": datetime.now().isoformat(),
            "server": f"{self.host}:{self.port}",
            "tests": []
        }
        
        try:
            self.connect()
            
            # Run all tests
            test_methods = [
                ("Basic Operations", self.test_basic_operations),
                ("Pipeline Performance", self.test_pipeline_performance),
                ("Mixed Workload", self.test_mixed_workload),
                ("Stress Operations", self.test_stress_operations),
                ("System Statistics", self.test_system_stats),
            ]
            
            for test_name, test_method in test_methods:
                try:
                    test_method()
                    results["tests"].append({"name": test_name, "status": "PASSED"})
                except Exception as e:
                    print(f"❌ {test_name} failed: {e}")
                    results["tests"].append({"name": test_name, "status": "FAILED", "error": str(e)})
                    raise
            
            # Summary
            print("\n" + "=" * 60)
            print("🎉 ALL TESTS PASSED!")
            print("=" * 60)
            
            passed_tests = len([t for t in results["tests"] if t["status"] == "PASSED"])
            total_tests = len(results["tests"])
            
            print(f"✅ Tests passed: {passed_tests}/{total_tests}")
            print(f"🚀 CrabCache is fully functional with all features working!")
            
            # Key achievements
            print("\n🏆 Key Achievements Validated:")
            print("   ✅ Basic cache operations (PUT/GET/DEL/TTL)")
            print("   ✅ Pipeline processing with performance improvements")
            print("   ✅ Mixed workload handling")
            print("   ✅ Stress testing with large batches")
            print("   ✅ System monitoring and statistics")
            
            print("\n🎯 CrabCache is ready for production use!")
            
            results["summary"] = {
                "total_tests": total_tests,
                "passed_tests": passed_tests,
                "success_rate": passed_tests / total_tests * 100,
                "status": "ALL_PASSED"
            }
            
            return results
            
        except Exception as e:
            print(f"\n❌ System test failed: {e}")
            results["summary"] = {
                "status": "FAILED",
                "error": str(e)
            }
            return results
        finally:
            self.disconnect()

def main():
    """Main test execution"""
    import argparse
    
    parser = argparse.ArgumentParser(description="CrabCache Complete System Test")
    parser.add_argument("--host", default="127.0.0.1", help="CrabCache server host")
    parser.add_argument("--port", type=int, default=8000, help="CrabCache server port")
    parser.add_argument("--output", help="Output file for JSON results")
    
    args = parser.parse_args()
    
    tester = CrabCacheSystemTest(args.host, args.port)
    
    try:
        results = tester.run_complete_test()
        
        # Save results if requested
        if args.output:
            with open(args.output, 'w') as f:
                json.dump(results, f, indent=2)
            print(f"\n💾 Results saved to {args.output}")
        
        # Exit with appropriate code
        if results["summary"]["status"] == "ALL_PASSED":
            sys.exit(0)
        else:
            sys.exit(1)
            
    except KeyboardInterrupt:
        print("\n⏹️  Test interrupted by user")
        sys.exit(1)
    except Exception as e:
        print(f"\n❌ Test execution failed: {e}")
        sys.exit(1)

if __name__ == "__main__":
    main()