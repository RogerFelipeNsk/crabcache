#!/usr/bin/env python3
"""
Ultra Low Latency Benchmark for CrabCache

This benchmark is specifically designed to achieve P99 < 1ms
by focusing on latency optimizations rather than throughput.
"""

import socket
import struct
import time
import statistics
import threading
from concurrent.futures import ThreadPoolExecutor
from dataclasses import dataclass
from typing import List, Tuple, Optional
import json
import numpy as np

# Binary protocol constants
CMD_PING = 0x01
CMD_PUT = 0x02
CMD_GET = 0x03
CMD_DEL = 0x04

RESP_OK = 0x10
RESP_PONG = 0x11
RESP_NULL = 0x12
RESP_ERROR = 0x13
RESP_VALUE = 0x14

@dataclass
class UltraLowLatencyConfig:
    host: str = "127.0.0.1"
    port: int = 7001
    connections: int = 5  # Very low concurrency for minimal latency
    operations_per_connection: int = 10000  # Many operations for good P99 measurement
    target_p99_ms: float = 1.0

class UltraLowLatencyClient:
    """Client optimized for ultra-low latency"""
    
    def __init__(self, host: str, port: int, client_id: int):
        self.host = host
        self.port = port
        self.client_id = client_id
        self.socket = None
        self.connected = False
    
    def connect_ultra_optimized(self) -> bool:
        """Connect with ultra-low latency optimizations"""
        try:
            self.socket = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
            
            # ULTRA-LOW LATENCY OPTIMIZATIONS
            self.socket.setsockopt(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)  # Disable Nagle
            self.socket.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)  # Reuse address
            
            # Smaller buffers for lower latency (trade throughput for latency)
            self.socket.setsockopt(socket.SOL_SOCKET, socket.SO_SNDBUF, 4096)   # 4KB send
            self.socket.setsockopt(socket.SOL_SOCKET, socket.SO_RCVBUF, 4096)   # 4KB recv
            
            # Very short timeout for immediate failure detection
            self.socket.settimeout(0.1)  # 100ms timeout
            
            self.socket.connect((self.host, self.port))
            
            # Validate with single PING
            self.socket.send(bytes([CMD_PING]))
            response = self.socket.recv(1)
            
            if len(response) == 1 and response[0] == RESP_PONG:
                self.connected = True
                return True
            return False
        except Exception:
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
    
    def run_ultra_low_latency_operations(self, operations: int) -> dict:
        """Run operations optimized for ultra-low latency"""
        results = {
            "successful_operations": 0,
            "failed_operations": 0,
            "latencies_ns": [],  # Nanosecond precision
            "latencies_ms": []   # Millisecond for analysis
        }
        
        # Pre-generate commands to minimize overhead
        ping_cmd = bytes([CMD_PING])
        
        # Simple key-value for PUT/GET tests
        test_key = f"ultra_key_{self.client_id}".encode()
        test_value = b"ultra_value"
        
        put_cmd = bytearray()
        put_cmd.append(CMD_PUT)
        put_cmd.extend(struct.pack('<I', len(test_key)))
        put_cmd.extend(test_key)
        put_cmd.extend(struct.pack('<I', len(test_value)))
        put_cmd.extend(test_value)
        put_cmd.append(0)  # No TTL
        
        get_cmd = bytearray()
        get_cmd.append(CMD_GET)
        get_cmd.extend(struct.pack('<I', len(test_key)))
        get_cmd.extend(test_key)
        
        # Store one value first
        try:
            self.socket.send(put_cmd)
            response = self.socket.recv(1)
            if len(response) != 1 or response[0] != RESP_OK:
                return results
        except:
            return results
        
        # Run ultra-low latency operations
        for i in range(operations):
            operation_type = i % 3  # 33% PING, 33% PUT, 33% GET
            
            try:
                # Use high-precision timing
                start_time = time.perf_counter_ns()
                success = False
                
                if operation_type == 0:  # PING
                    self.socket.send(ping_cmd)
                    response = self.socket.recv(1)
                    success = len(response) == 1 and response[0] == RESP_PONG
                
                elif operation_type == 1:  # PUT
                    # Use unique key for each PUT to avoid caching effects
                    unique_key = f"ultra_key_{self.client_id}_{i}".encode()
                    unique_put_cmd = bytearray()
                    unique_put_cmd.append(CMD_PUT)
                    unique_put_cmd.extend(struct.pack('<I', len(unique_key)))
                    unique_put_cmd.extend(unique_key)
                    unique_put_cmd.extend(struct.pack('<I', len(test_value)))
                    unique_put_cmd.extend(test_value)
                    unique_put_cmd.append(0)  # No TTL
                    
                    self.socket.send(unique_put_cmd)
                    response = self.socket.recv(1)
                    success = len(response) == 1 and response[0] == RESP_OK
                
                else:  # GET
                    self.socket.send(get_cmd)
                    response_type = self.socket.recv(1)
                    
                    if len(response_type) == 1:
                        if response_type[0] == RESP_NULL:
                            success = True
                        elif response_type[0] == RESP_VALUE:
                            # Read value length and value
                            len_bytes = self.socket.recv(4)
                            if len(len_bytes) == 4:
                                value_len = struct.unpack('<I', len_bytes)[0]
                                if value_len <= 1024:  # Reasonable size
                                    value = self.socket.recv(value_len)
                                    success = len(value) == value_len
                
                end_time = time.perf_counter_ns()
                latency_ns = end_time - start_time
                latency_ms = latency_ns / 1_000_000  # Convert to milliseconds
                
                if success:
                    results["successful_operations"] += 1
                    results["latencies_ns"].append(latency_ns)
                    results["latencies_ms"].append(latency_ms)
                else:
                    results["failed_operations"] += 1
                    
            except Exception:
                results["failed_operations"] += 1
        
        return results

class UltraLowLatencyBenchmark:
    """Benchmark focused on achieving P99 < 1ms"""
    
    def __init__(self, config: UltraLowLatencyConfig):
        self.config = config
    
    def run_ultra_low_latency_benchmark(self):
        """Run ultra-low latency benchmark"""
        print("⚡ CrabCache ULTRA LOW LATENCY Benchmark")
        print("=" * 50)
        print(f"🎯 TARGET: P99 < {self.config.target_p99_ms}ms")
        print(f"Server: {self.config.host}:{self.config.port}")
        print(f"Connections: {self.config.connections} (optimized for latency)")
        print(f"Operations per connection: {self.config.operations_per_connection}")
        print(f"Total operations: {self.config.connections * self.config.operations_per_connection:,}")
        print()
        
        # Test connection
        if not self.test_ultra_low_latency_connection():
            print("❌ Ultra-low latency connection test failed")
            return
        
        # Run benchmark
        print("⚡ Running ultra-low latency benchmark...")
        start_time = time.perf_counter()
        
        results = self.run_ultra_low_latency_workers()
        
        end_time = time.perf_counter()
        duration = end_time - start_time
        
        # Calculate detailed latency metrics
        self.analyze_ultra_low_latency_results(results, duration)
    
    def test_ultra_low_latency_connection(self) -> bool:
        """Test ultra-low latency connection"""
        print("⚡ Testing ultra-low latency connection...")
        
        client = UltraLowLatencyClient(self.config.host, self.config.port, 0)
        if client.connect_ultra_optimized():
            # Test a few operations to measure baseline latency
            latencies = []
            for _ in range(10):
                start = time.perf_counter_ns()
                client.socket.send(bytes([CMD_PING]))
                response = client.socket.recv(1)
                end = time.perf_counter_ns()
                
                if len(response) == 1 and response[0] == RESP_PONG:
                    latencies.append((end - start) / 1_000_000)  # Convert to ms
            
            client.disconnect()
            
            if latencies:
                avg_latency = sum(latencies) / len(latencies)
                min_latency = min(latencies)
                max_latency = max(latencies)
                
                print(f"  ✅ Connection successful")
                print(f"  📊 Baseline latency: {avg_latency:.3f}ms avg, {min_latency:.3f}ms min, {max_latency:.3f}ms max")
                
                if max_latency < self.config.target_p99_ms:
                    print(f"  🎯 Baseline already under target ({max_latency:.3f}ms < {self.config.target_p99_ms}ms)")
                else:
                    print(f"  ⚠️  Baseline above target ({max_latency:.3f}ms > {self.config.target_p99_ms}ms)")
                
                return True
        
        print("  ❌ Connection failed")
        return False
    
    def run_ultra_low_latency_workers(self) -> dict:
        """Run ultra-low latency workers"""
        combined_results = {
            "successful_operations": 0,
            "failed_operations": 0,
            "latencies_ms": []
        }
        
        def worker(client_id: int):
            client = UltraLowLatencyClient(self.config.host, self.config.port, client_id)
            
            if not client.connect_ultra_optimized():
                return {
                    "successful_operations": 0,
                    "failed_operations": self.config.operations_per_connection,
                    "latencies_ms": []
                }
            
            try:
                results = client.run_ultra_low_latency_operations(self.config.operations_per_connection)
                client.disconnect()
                return results
            except Exception:
                client.disconnect()
                return {
                    "successful_operations": 0,
                    "failed_operations": self.config.operations_per_connection,
                    "latencies_ms": []
                }
        
        # Use minimal concurrency for lowest latency
        with ThreadPoolExecutor(max_workers=self.config.connections) as executor:
            futures = [executor.submit(worker, i) for i in range(self.config.connections)]
            
            for future in futures:
                worker_results = future.result()
                combined_results["successful_operations"] += worker_results["successful_operations"]
                combined_results["failed_operations"] += worker_results["failed_operations"]
                combined_results["latencies_ms"].extend(worker_results["latencies_ms"])
        
        return combined_results
    
    def analyze_ultra_low_latency_results(self, results: dict, duration: float):
        """Analyze ultra-low latency results with detailed percentiles"""
        print("📊 ULTRA LOW LATENCY Results:")
        print("=" * 40)
        
        total_operations = results["successful_operations"] + results["failed_operations"]
        throughput = results["successful_operations"] / duration if duration > 0 else 0
        success_rate = results["successful_operations"] / total_operations * 100 if total_operations > 0 else 0
        
        print(f"Total Operations: {total_operations:,}")
        print(f"Successful Operations: {results['successful_operations']:,}")
        print(f"Failed Operations: {results['failed_operations']:,}")
        print(f"Success Rate: {success_rate:.1f}%")
        print(f"Duration: {duration:.2f}s")
        print(f"Throughput: {throughput:,.0f} ops/sec")
        print()
        
        if results["latencies_ms"]:
            latencies = np.array(results["latencies_ms"])
            
            # Calculate detailed percentiles
            percentiles = [50, 90, 95, 99, 99.9, 99.99]
            percentile_values = np.percentile(latencies, percentiles)
            
            print("📈 Detailed Latency Analysis:")
            print("-" * 30)
            print(f"Min:     {np.min(latencies):.3f}ms")
            print(f"Mean:    {np.mean(latencies):.3f}ms")
            print(f"Max:     {np.max(latencies):.3f}ms")
            print(f"Std Dev: {np.std(latencies):.3f}ms")
            print()
            
            print("Percentiles:")
            for p, value in zip(percentiles, percentile_values):
                status = "✅" if value < self.config.target_p99_ms or p < 99 else "❌"
                print(f"  P{p:>5}: {value:>7.3f}ms {status}")
            
            print()
            
            # P99 goal assessment
            p99_value = percentile_values[3]  # P99 is at index 3
            
            print("🎯 P99 Goal Assessment:")
            if p99_value < self.config.target_p99_ms:
                margin = self.config.target_p99_ms - p99_value
                print(f"  🏆 P99 GOAL ACHIEVED! ({p99_value:.3f}ms < {self.config.target_p99_ms}ms)")
                print(f"  🎉 Margin: {margin:.3f}ms under target")
            else:
                excess = p99_value - self.config.target_p99_ms
                print(f"  ❌ P99 goal not achieved ({p99_value:.3f}ms > {self.config.target_p99_ms}ms)")
                print(f"  📈 Need to reduce by: {excess:.3f}ms ({excess/self.config.target_p99_ms*100:.1f}%)")
            
            print()
            
            # Latency distribution analysis
            self.analyze_latency_distribution(latencies)
            
            # Save results
            self.save_ultra_low_latency_results(results, duration, percentile_values, p99_value)
        else:
            print("❌ No latency data collected")
    
    def analyze_latency_distribution(self, latencies: np.ndarray):
        """Analyze latency distribution for optimization insights"""
        print("🔍 Latency Distribution Analysis:")
        print("-" * 35)
        
        # Latency buckets
        buckets = [
            (0.0, 0.1, "Ultra-fast"),
            (0.1, 0.5, "Very fast"),
            (0.5, 1.0, "Fast"),
            (1.0, 2.0, "Acceptable"),
            (2.0, 5.0, "Slow"),
            (5.0, float('inf'), "Very slow")
        ]
        
        total_ops = len(latencies)
        
        for min_lat, max_lat, label in buckets:
            if max_lat == float('inf'):
                count = np.sum(latencies >= min_lat)
            else:
                count = np.sum((latencies >= min_lat) & (latencies < max_lat))
            
            percentage = count / total_ops * 100
            
            if count > 0:
                print(f"  {label:>12} ({min_lat:>3.1f}-{max_lat:>3.1f}ms): {count:>6,} ops ({percentage:>5.1f}%)")
        
        print()
        
        # Optimization recommendations
        p99_value = np.percentile(latencies, 99)
        p95_value = np.percentile(latencies, 95)
        
        print("💡 Optimization Recommendations:")
        if p99_value > 1.0:
            print("  🔧 P99 > 1ms: Consider reducing connection pool size")
            print("  🔧 Implement connection pre-warming")
            print("  🔧 Optimize memory allocation patterns")
        
        if p95_value > 0.5:
            print("  🔧 P95 > 0.5ms: Review lock contention")
            print("  🔧 Consider CPU affinity for server process")
        
        slow_ops = np.sum(latencies > 2.0)
        if slow_ops > 0:
            slow_percentage = slow_ops / total_ops * 100
            print(f"  ⚠️  {slow_ops:,} operations > 2ms ({slow_percentage:.1f}%)")
            print("  🔧 Investigate outliers and GC pauses")
    
    def save_ultra_low_latency_results(self, results: dict, duration: float, percentiles: np.ndarray, p99_value: float):
        """Save ultra-low latency results"""
        timestamp = time.strftime("%Y%m%d_%H%M%S")
        filename = f"crabcache/benchmark_results/ultra_low_latency_results_{timestamp}.json"
        
        data = {
            "timestamp": timestamp,
            "config": {
                "connections": self.config.connections,
                "operations_per_connection": self.config.operations_per_connection,
                "target_p99_ms": self.config.target_p99_ms,
            },
            "results": {
                "total_operations": results["successful_operations"] + results["failed_operations"],
                "successful_operations": results["successful_operations"],
                "failed_operations": results["failed_operations"],
                "duration_seconds": duration,
                "throughput_ops_per_sec": results["successful_operations"] / duration if duration > 0 else 0,
            },
            "latency_metrics": {
                "min_ms": float(np.min(results["latencies_ms"])) if results["latencies_ms"] else 0,
                "mean_ms": float(np.mean(results["latencies_ms"])) if results["latencies_ms"] else 0,
                "max_ms": float(np.max(results["latencies_ms"])) if results["latencies_ms"] else 0,
                "std_dev_ms": float(np.std(results["latencies_ms"])) if results["latencies_ms"] else 0,
                "p50_ms": float(percentiles[0]) if len(percentiles) > 0 else 0,
                "p90_ms": float(percentiles[1]) if len(percentiles) > 1 else 0,
                "p95_ms": float(percentiles[2]) if len(percentiles) > 2 else 0,
                "p99_ms": float(percentiles[3]) if len(percentiles) > 3 else 0,
                "p99_9_ms": float(percentiles[4]) if len(percentiles) > 4 else 0,
                "p99_99_ms": float(percentiles[5]) if len(percentiles) > 5 else 0,
            },
            "goal_assessment": {
                "target_p99_ms": self.config.target_p99_ms,
                "actual_p99_ms": float(p99_value),
                "goal_achieved": bool(p99_value < self.config.target_p99_ms),
                "margin_ms": float(self.config.target_p99_ms - p99_value),
            }
        }
        
        with open(filename, 'w') as f:
            json.dump(data, f, indent=2)
        
        print(f"💾 Ultra-low latency results saved to: {filename}")

def main():
    print("⚡ Starting CrabCache Ultra Low Latency Benchmark...")
    print("🎯 Goal: Achieve P99 < 1ms")
    print()
    
    config = UltraLowLatencyConfig(
        host="127.0.0.1",
        port=7001,
        connections=5,  # Very low concurrency for minimal latency
        operations_per_connection=10000,  # Many operations for good P99 measurement
        target_p99_ms=1.0
    )
    
    benchmark = UltraLowLatencyBenchmark(config)
    benchmark.run_ultra_low_latency_benchmark()

if __name__ == "__main__":
    main()