#!/usr/bin/env python3
"""
Connection Scaling Test

This script tests CrabCache with increasing numbers of connections
to find the optimal connection count and identify scaling limits.
"""

import socket
import struct
import time
import threading
from concurrent.futures import ThreadPoolExecutor
import numpy as np
import json
from dataclasses import dataclass
from typing import List, Dict, Any

# Binary protocol constants
CMD_PING = 0x01
RESP_PONG = 0x11

@dataclass
class ScalingTestConfig:
    host: str = "127.0.0.1"
    port: int = 7001
    operations_per_connection: int = 1000  # Reduced for faster testing
    test_duration: int = 10  # 10 seconds per test
    connection_counts: List[int] = None

    def __post_init__(self):
        if self.connection_counts is None:
            # Test with increasing connection counts
            self.connection_counts = [1, 2, 5, 10, 20, 30, 50, 75, 100]

class ScalingClient:
    """Simple client for connection scaling tests"""
    
    def __init__(self, host: str, port: int, client_id: int):
        self.host = host
        self.port = port
        self.client_id = client_id
        self.socket = None
        self.connected = False
    
    def connect(self) -> bool:
        """Connect with optimized settings"""
        try:
            self.socket = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
            
            # Optimized TCP settings
            self.socket.setsockopt(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)
            self.socket.setsockopt(socket.SOL_SOCKET, socket.SO_RCVBUF, 8192)
            self.socket.setsockopt(socket.SOL_SOCKET, socket.SO_SNDBUF, 4096)
            self.socket.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
            self.socket.settimeout(5.0)  # 5 second timeout
            
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
    
    def run_ping_test(self, operations: int) -> Dict:
        """Run simple PING test"""
        results = {
            "client_id": self.client_id,
            "successful_operations": 0,
            "failed_operations": 0,
            "latencies_ms": [],
            "errors": [],
        }
        
        if not self.connected:
            results["errors"].append("Not connected")
            results["failed_operations"] = operations
            return results
        
        ping_cmd = bytes([CMD_PING])
        
        for i in range(operations):
            try:
                start_time = time.perf_counter_ns()
                
                self.socket.send(ping_cmd)
                response = self.socket.recv(1)
                
                end_time = time.perf_counter_ns()
                
                if len(response) == 1 and response[0] == RESP_PONG:
                    latency_ms = (end_time - start_time) / 1_000_000
                    results["latencies_ms"].append(latency_ms)
                    results["successful_operations"] += 1
                else:
                    results["failed_operations"] += 1
                    
            except Exception as e:
                results["failed_operations"] += 1
                results["errors"].append(f"Op {i}: {str(e)}")
                
                # If we get too many consecutive errors, stop
                if len(results["errors"]) > 10:
                    break
        
        return results

class ConnectionScalingTest:
    """Test to find optimal connection count"""
    
    def __init__(self, config: ScalingTestConfig):
        self.config = config
    
    def run_scaling_test(self):
        """Run connection scaling test"""
        print("🔍 CrabCache Connection Scaling Test")
        print("=" * 40)
        print("🎯 GOAL: Find optimal connection count")
        print()
        
        results = {}
        
        for conn_count in self.config.connection_counts:
            print(f"📊 Testing with {conn_count} connections...")
            
            result = self.test_connection_count(conn_count)
            results[conn_count] = result
            
            # Print immediate results
            success_rate = result["success_rate"]
            throughput = result["throughput"]
            avg_latency = result["avg_latency"]
            
            status = "✅" if success_rate > 95 else "⚠️" if success_rate > 80 else "❌"
            
            print(f"  Success Rate: {success_rate:.1f}% {status}")
            print(f"  Throughput: {throughput:,.0f} ops/sec")
            print(f"  Avg Latency: {avg_latency:.3f}ms")
            
            # If success rate drops below 50%, stop testing higher counts
            if success_rate < 50:
                print(f"  ⚠️  Success rate too low, stopping at {conn_count} connections")
                break
            
            print()
        
        # Analyze results
        self.analyze_scaling_results(results)
    
    def test_connection_count(self, connection_count: int) -> Dict:
        """Test specific connection count"""
        def worker(client_id: int):
            client = ScalingClient(self.config.host, self.config.port, client_id)
            
            if not client.connect():
                return {
                    "client_id": client_id,
                    "successful_operations": 0,
                    "failed_operations": self.config.operations_per_connection,
                    "latencies_ms": [],
                    "errors": ["Connection failed"],
                }
            
            try:
                result = client.run_ping_test(self.config.operations_per_connection)
                client.disconnect()
                return result
            except Exception as e:
                client.disconnect()
                return {
                    "client_id": client_id,
                    "successful_operations": 0,
                    "failed_operations": self.config.operations_per_connection,
                    "latencies_ms": [],
                    "errors": [str(e)],
                }
        
        # Run test
        start_time = time.perf_counter()
        
        with ThreadPoolExecutor(max_workers=connection_count) as executor:
            futures = [executor.submit(worker, i) for i in range(connection_count)]
            worker_results = [future.result() for future in futures]
        
        end_time = time.perf_counter()
        duration = end_time - start_time
        
        # Combine results
        total_successful = sum(r["successful_operations"] for r in worker_results)
        total_failed = sum(r["failed_operations"] for r in worker_results)
        all_latencies = []
        all_errors = []
        
        for result in worker_results:
            all_latencies.extend(result["latencies_ms"])
            all_errors.extend(result["errors"])
        
        # Calculate metrics
        total_operations = total_successful + total_failed
        success_rate = (total_successful / total_operations * 100) if total_operations > 0 else 0
        throughput = total_successful / duration if duration > 0 else 0
        avg_latency = np.mean(all_latencies) if all_latencies else 0
        
        # Calculate percentiles
        percentiles = {}
        if all_latencies:
            latencies = np.array(all_latencies)
            percentiles = {
                "p50": float(np.percentile(latencies, 50)),
                "p95": float(np.percentile(latencies, 95)),
                "p99": float(np.percentile(latencies, 99)),
            }
        
        return {
            "connection_count": connection_count,
            "duration": duration,
            "total_successful": total_successful,
            "total_failed": total_failed,
            "success_rate": success_rate,
            "throughput": throughput,
            "avg_latency": avg_latency,
            "percentiles": percentiles,
            "error_count": len(all_errors),
            "unique_errors": list(set(all_errors[:10])),  # First 10 unique errors
        }
    
    def analyze_scaling_results(self, results: Dict):
        """Analyze scaling test results"""
        print("📊 Connection Scaling Analysis:")
        print("=" * 35)
        
        print(f"{'Connections':<12} {'Success%':<9} {'Throughput':<12} {'P99 Latency':<12} {'Status'}")
        print("-" * 60)
        
        best_throughput = 0
        best_connection_count = 0
        optimal_connection_count = 0
        
        for conn_count, result in results.items():
            success_rate = result["success_rate"]
            throughput = result["throughput"]
            p99 = result["percentiles"].get("p99", 0)
            
            # Track best throughput
            if throughput > best_throughput:
                best_throughput = throughput
                best_connection_count = conn_count
            
            # Find optimal (good success rate + good throughput)
            if success_rate > 95 and throughput > best_throughput * 0.8:
                optimal_connection_count = conn_count
            
            # Status indicators
            if success_rate > 95 and p99 < 1.0:
                status = "✅ EXCELLENT"
            elif success_rate > 90 and p99 < 2.0:
                status = "✅ GOOD"
            elif success_rate > 80:
                status = "⚠️ ACCEPTABLE"
            else:
                status = "❌ POOR"
            
            print(f"{conn_count:<12} {success_rate:<8.1f}% {throughput:<11,.0f} {p99:<11.3f}ms {status}")
        
        print()
        print("🎯 Scaling Analysis Summary:")
        print(f"  Best Throughput: {best_throughput:,.0f} ops/sec with {best_connection_count} connections")
        
        if optimal_connection_count > 0:
            print(f"  Optimal Config: {optimal_connection_count} connections (best balance)")
        else:
            print(f"  Optimal Config: {best_connection_count} connections (max throughput)")
        
        # Recommendations
        print()
        print("💡 Recommendations:")
        
        if best_throughput < 1000:
            print("  ❌ Very low throughput - check server implementation")
            print("  🔧 Possible issues: connection handling, protocol parsing, or resource limits")
        elif best_throughput < 10000:
            print("  ⚠️  Low throughput - optimization needed")
            print("  🔧 Consider: async I/O, connection pooling, or protocol optimization")
        elif best_throughput < 25000:
            print("  ✅ Moderate throughput - good baseline")
            print("  🔧 Consider: pipelining implementation for higher throughput")
        else:
            print("  🚀 Excellent throughput!")
            print("  🔧 Ready for pipelining implementation")
        
        # Connection scaling recommendations
        max_good_connections = max([k for k, v in results.items() if v["success_rate"] > 90], default=1)
        print(f"  📊 Max reliable connections: {max_good_connections}")
        
        if max_good_connections < 10:
            print("  ⚠️  Low connection limit - may need async architecture")
        elif max_good_connections < 50:
            print("  ✅ Good connection scaling")
        else:
            print("  🚀 Excellent connection scaling")
        
        # Save results
        self.save_scaling_results(results)
    
    def save_scaling_results(self, results: Dict):
        """Save scaling test results"""
        timestamp = time.strftime("%Y%m%d_%H%M%S")
        filename = f"benchmark_results/connection_scaling_{timestamp}.json"
        
        summary_data = {
            "timestamp": timestamp,
            "config": {
                "operations_per_connection": self.config.operations_per_connection,
                "test_duration": self.config.test_duration,
                "connection_counts_tested": list(results.keys()),
            },
            "results": results,
        }
        
        try:
            with open(filename, 'w') as f:
                json.dump(summary_data, f, indent=2)
            print(f"💾 Results saved to: {filename}")
        except Exception as e:
            print(f"⚠️  Could not save results: {e}")

def main():
    print("🔍 Starting CrabCache Connection Scaling Test...")
    print("🎯 Goal: Find optimal connection count and scaling limits")
    print()
    
    config = ScalingTestConfig(
        host="127.0.0.1",
        port=7001,
        operations_per_connection=1000,  # 1k ops per connection for faster testing
        test_duration=10,
        connection_counts=[1, 2, 5, 10, 15, 20, 25, 30, 40, 50, 75, 100]
    )
    
    test = ConnectionScalingTest(config)
    test.run_scaling_test()
    
    print("\n🎊 Connection Scaling Test Complete!")
    print("📊 Use results to optimize connection handling and find sweet spot")

if __name__ == "__main__":
    main()