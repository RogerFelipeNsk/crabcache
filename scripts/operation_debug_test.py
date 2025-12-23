#!/usr/bin/env python3
"""
Operation Debug Test

This script tests individual operations to identify which ones are causing
performance issues in the realistic Redis test.
"""

import socket
import struct
import time
import threading
from concurrent.futures import ThreadPoolExecutor
import numpy as np
from dataclasses import dataclass
from typing import List, Dict, Any

# Binary protocol constants
CMD_PING = 0x01
CMD_PUT = 0x02
CMD_GET = 0x03

RESP_OK = 0x10
RESP_PONG = 0x11
RESP_VALUE = 0x14
RESP_NULL = 0x12

@dataclass
class DebugTestConfig:
    host: str = "127.0.0.1"
    port: int = 7001
    connections: int = 10
    operations_per_test: int = 1000
    data_size: int = 64

class DebugClient:
    """Simple client for debugging operations"""
    
    def __init__(self, host: str, port: int, client_id: int):
        self.host = host
        self.port = port
        self.client_id = client_id
        self.socket = None
        self.connected = False
    
    def connect(self) -> bool:
        """Connect with simple settings"""
        try:
            self.socket = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
            self.socket.setsockopt(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)
            self.socket.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
            self.socket.settimeout(10.0)
            
            self.socket.connect((self.host, self.port))
            self.connected = True
            return True
            
        except Exception as e:
            print(f"Client {self.client_id} connection failed: {e}")
            return False
    
    def disconnect(self):
        """Clean disconnect"""
        if self.socket:
            try:
                self.socket.close()
            except:
                pass
            self.socket = None
            self.connected = False
    
    def test_ping_operations(self, count: int) -> Dict:
        """Test PING operations only"""
        results = {"successful": 0, "failed": 0, "latencies": [], "errors": []}
        
        if not self.connected:
            results["failed"] = count
            results["errors"].append("Not connected")
            return results
        
        ping_cmd = bytes([CMD_PING])
        
        for i in range(count):
            try:
                start_time = time.perf_counter_ns()
                self.socket.send(ping_cmd)
                response = self.socket.recv(1)
                end_time = time.perf_counter_ns()
                
                if len(response) == 1 and response[0] == RESP_PONG:
                    latency_ms = (end_time - start_time) / 1_000_000
                    results["latencies"].append(latency_ms)
                    results["successful"] += 1
                else:
                    results["failed"] += 1
                    results["errors"].append(f"Invalid PING response: {response}")
                    
            except Exception as e:
                results["failed"] += 1
                results["errors"].append(f"PING error {i}: {str(e)}")
                if len(results["errors"]) > 10:
                    break
        
        return results
    
    def test_put_operations(self, count: int, data_size: int) -> Dict:
        """Test PUT operations only"""
        results = {"successful": 0, "failed": 0, "latencies": [], "errors": []}
        
        if not self.connected:
            results["failed"] = count
            results["errors"].append("Not connected")
            return results
        
        test_value = b'x' * data_size
        
        for i in range(count):
            try:
                # Create PUT command
                key = f"debug_key_{self.client_id}_{i}".encode()
                cmd = bytearray()
                cmd.append(CMD_PUT)
                cmd.extend(struct.pack('<I', len(key)))
                cmd.extend(key)
                cmd.extend(struct.pack('<I', len(test_value)))
                cmd.extend(test_value)
                cmd.append(0)  # No TTL
                
                start_time = time.perf_counter_ns()
                self.socket.send(cmd)
                response = self.socket.recv(1)
                end_time = time.perf_counter_ns()
                
                if len(response) == 1 and response[0] == RESP_OK:
                    latency_ms = (end_time - start_time) / 1_000_000
                    results["latencies"].append(latency_ms)
                    results["successful"] += 1
                else:
                    results["failed"] += 1
                    results["errors"].append(f"Invalid PUT response: {response}")
                    
            except Exception as e:
                results["failed"] += 1
                results["errors"].append(f"PUT error {i}: {str(e)}")
                if len(results["errors"]) > 10:
                    break
        
        return results
    
    def test_get_operations(self, count: int, data_size: int) -> Dict:
        """Test GET operations (requires pre-populated keys)"""
        results = {"successful": 0, "failed": 0, "latencies": [], "errors": []}
        
        if not self.connected:
            results["failed"] = count
            results["errors"].append("Not connected")
            return results
        
        # First, populate some keys
        test_value = b'x' * data_size
        for i in range(min(100, count)):
            try:
                key = f"get_test_key_{self.client_id}_{i}".encode()
                cmd = bytearray()
                cmd.append(CMD_PUT)
                cmd.extend(struct.pack('<I', len(key)))
                cmd.extend(key)
                cmd.extend(struct.pack('<I', len(test_value)))
                cmd.extend(test_value)
                cmd.append(0)  # No TTL
                
                self.socket.send(cmd)
                response = self.socket.recv(1)  # Consume PUT response
            except:
                pass  # Ignore setup errors
        
        # Now test GET operations
        for i in range(count):
            try:
                key_idx = i % 100
                key = f"get_test_key_{self.client_id}_{key_idx}".encode()
                
                # Create GET command
                cmd = bytearray()
                cmd.append(CMD_GET)
                cmd.extend(struct.pack('<I', len(key)))
                cmd.extend(key)
                
                start_time = time.perf_counter_ns()
                self.socket.send(cmd)
                response = self.socket.recv(1)
                
                if len(response) == 1:
                    if response[0] == RESP_VALUE:
                        # Read value length and value
                        len_bytes = self.socket.recv(4)
                        if len(len_bytes) == 4:
                            value_len = struct.unpack('<I', len_bytes)[0]
                            if value_len <= 1024:  # Reasonable limit
                                value = self.socket.recv(value_len)
                                if len(value) == value_len:
                                    end_time = time.perf_counter_ns()
                                    latency_ms = (end_time - start_time) / 1_000_000
                                    results["latencies"].append(latency_ms)
                                    results["successful"] += 1
                                    continue
                    elif response[0] == RESP_NULL:
                        # NULL response is also valid
                        end_time = time.perf_counter_ns()
                        latency_ms = (end_time - start_time) / 1_000_000
                        results["latencies"].append(latency_ms)
                        results["successful"] += 1
                        continue
                
                results["failed"] += 1
                results["errors"].append(f"Invalid GET response: {response}")
                    
            except Exception as e:
                results["failed"] += 1
                results["errors"].append(f"GET error {i}: {str(e)}")
                if len(results["errors"]) > 10:
                    break
        
        return results

class OperationDebugTest:
    """Test individual operations to debug performance issues"""
    
    def __init__(self, config: DebugTestConfig):
        self.config = config
    
    def run_debug_tests(self):
        """Run debug tests for each operation type"""
        print("🔍 CrabCache Operation Debug Test")
        print("=" * 35)
        print("🎯 GOAL: Identify which operations cause performance issues")
        print()
        
        operations_to_test = [
            ("PING", self.test_operation_type, "ping"),
            ("PUT", self.test_operation_type, "put"),
            ("GET", self.test_operation_type, "get"),
        ]
        
        results = {}
        
        for op_name, test_func, op_type in operations_to_test:
            print(f"📊 Testing {op_name} operations...")
            result = test_func(op_type)
            results[op_name] = result
            
            # Print immediate results
            success_rate = result["success_rate"]
            throughput = result["throughput"]
            avg_latency = result["avg_latency"]
            
            status = "✅" if success_rate > 95 else "⚠️" if success_rate > 80 else "❌"
            
            print(f"  Success Rate: {success_rate:.1f}% {status}")
            print(f"  Throughput: {throughput:,.0f} ops/sec")
            print(f"  Avg Latency: {avg_latency:.3f}ms")
            
            if result["error_samples"]:
                print(f"  Sample Errors: {result['error_samples'][:3]}")
            
            print()
        
        # Compare results
        self.compare_operation_performance(results)
    
    def test_operation_type(self, operation_type: str) -> Dict:
        """Test specific operation type"""
        def worker(client_id: int):
            client = DebugClient(self.config.host, self.config.port, client_id)
            
            if not client.connect():
                return {
                    "successful": 0,
                    "failed": self.config.operations_per_test,
                    "latencies": [],
                    "errors": ["Connection failed"],
                }
            
            try:
                if operation_type == "ping":
                    result = client.test_ping_operations(self.config.operations_per_test)
                elif operation_type == "put":
                    result = client.test_put_operations(self.config.operations_per_test, self.config.data_size)
                elif operation_type == "get":
                    result = client.test_get_operations(self.config.operations_per_test, self.config.data_size)
                else:
                    result = {"successful": 0, "failed": self.config.operations_per_test, "latencies": [], "errors": ["Unknown operation"]}
                
                client.disconnect()
                return result
            except Exception as e:
                client.disconnect()
                return {
                    "successful": 0,
                    "failed": self.config.operations_per_test,
                    "latencies": [],
                    "errors": [str(e)],
                }
        
        # Run test
        start_time = time.perf_counter()
        
        with ThreadPoolExecutor(max_workers=self.config.connections) as executor:
            futures = [executor.submit(worker, i) for i in range(self.config.connections)]
            worker_results = [future.result() for future in futures]
        
        end_time = time.perf_counter()
        duration = end_time - start_time
        
        # Combine results
        total_successful = sum(r["successful"] for r in worker_results)
        total_failed = sum(r["failed"] for r in worker_results)
        all_latencies = []
        all_errors = []
        
        for result in worker_results:
            all_latencies.extend(result["latencies"])
            all_errors.extend(result["errors"])
        
        # Calculate metrics
        total_operations = total_successful + total_failed
        success_rate = (total_successful / total_operations * 100) if total_operations > 0 else 0
        throughput = total_successful / duration if duration > 0 else 0
        avg_latency = np.mean(all_latencies) if all_latencies else 0
        
        return {
            "operation_type": operation_type,
            "duration": duration,
            "total_successful": total_successful,
            "total_failed": total_failed,
            "success_rate": success_rate,
            "throughput": throughput,
            "avg_latency": avg_latency,
            "error_count": len(all_errors),
            "error_samples": list(set(all_errors[:10])),  # Unique error samples
        }
    
    def compare_operation_performance(self, results: Dict):
        """Compare performance across operation types"""
        print("📊 Operation Performance Comparison:")
        print("=" * 40)
        
        print(f"{'Operation':<10} {'Success%':<9} {'Throughput':<12} {'Avg Latency':<12} {'Status'}")
        print("-" * 55)
        
        best_throughput = 0
        worst_operation = ""
        
        for op_name, result in results.items():
            success_rate = result["success_rate"]
            throughput = result["throughput"]
            avg_latency = result["avg_latency"]
            
            if throughput > best_throughput:
                best_throughput = throughput
            
            if success_rate < 50:
                worst_operation = op_name
            
            # Status
            if success_rate > 95 and avg_latency < 1.0:
                status = "✅ EXCELLENT"
            elif success_rate > 90 and avg_latency < 2.0:
                status = "✅ GOOD"
            elif success_rate > 80:
                status = "⚠️ ACCEPTABLE"
            else:
                status = "❌ POOR"
            
            print(f"{op_name:<10} {success_rate:<8.1f}% {throughput:<11,.0f} {avg_latency:<11.3f}ms {status}")
        
        print()
        print("🎯 Analysis:")
        
        if worst_operation:
            print(f"  ❌ Problem Operation: {worst_operation}")
            print(f"     Sample Errors: {results[worst_operation]['error_samples'][:3]}")
        
        if best_throughput > 20000:
            print(f"  ✅ Best Performance: {best_throughput:,.0f} ops/sec")
            print(f"     CrabCache can handle high throughput with simple operations")
        elif best_throughput > 10000:
            print(f"  ⚠️  Moderate Performance: {best_throughput:,.0f} ops/sec")
            print(f"     Some operations may need optimization")
        else:
            print(f"  ❌ Low Performance: {best_throughput:,.0f} ops/sec")
            print(f"     Significant issues detected")
        
        # Recommendations
        print()
        print("💡 Recommendations:")
        
        ping_perf = results.get("PING", {}).get("throughput", 0)
        put_perf = results.get("PUT", {}).get("throughput", 0)
        get_perf = results.get("GET", {}).get("throughput", 0)
        
        if ping_perf > put_perf * 2:
            print("  🔧 PUT operations are significantly slower than PING")
            print("     Check: binary protocol parsing, storage operations")
        
        if ping_perf > get_perf * 2:
            print("  🔧 GET operations are significantly slower than PING")
            print("     Check: key lookup, value serialization, response handling")
        
        if put_perf > 0 and get_perf > 0 and abs(put_perf - get_perf) / max(put_perf, get_perf) > 0.5:
            print("  🔧 PUT and GET performance are very different")
            print("     Check: storage vs retrieval optimization balance")

def main():
    print("🔍 Starting CrabCache Operation Debug Test...")
    print("🎯 Goal: Identify which operations cause performance issues")
    print()
    
    config = DebugTestConfig(
        host="127.0.0.1",
        port=7001,
        connections=10,
        operations_per_test=1000,  # 1k ops per operation type
        data_size=64,
    )
    
    test = OperationDebugTest(config)
    test.run_debug_tests()
    
    print("\n🎊 Operation Debug Test Complete!")
    print("📊 Use results to identify and fix performance bottlenecks")

if __name__ == "__main__":
    main()