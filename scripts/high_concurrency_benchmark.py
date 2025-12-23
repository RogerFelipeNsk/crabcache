#!/usr/bin/env python3
"""
High Concurrency Benchmark for CrabCache Phase 3

This benchmark tests with higher concurrency to maximize throughput
and reach the Redis performance target.
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
class HighConcurrencyConfig:
    host: str = "127.0.0.1"
    port: int = 7001
    connections: int = 50  # Higher concurrency
    operations_per_connection: int = 2000  # More operations
    test_duration: int = 30

class OptimizedClient:
    """Optimized client for high concurrency testing"""
    
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
            # Optimize for high throughput
            self.socket.setsockopt(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)
            self.socket.setsockopt(socket.SOL_SOCKET, socket.SO_SNDBUF, 32768)  # 32KB
            self.socket.setsockopt(socket.SOL_SOCKET, socket.SO_RCVBUF, 32768)  # 32KB
            self.socket.settimeout(2.0)  # Shorter timeout for high concurrency
            self.socket.connect((self.host, self.port))
            
            # Quick validation
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
    
    def run_high_throughput_operations(self, operations: int) -> dict:
        """Run operations optimized for maximum throughput"""
        results = {
            "successful_operations": 0,
            "failed_operations": 0,
            "latencies": []
        }
        
        # Pre-generate data to minimize overhead
        ping_cmd = bytes([CMD_PING])
        
        # PUT command template
        key_template = f"hc_key_{self.client_id}_"
        value_template = b"high_concurrency_value_" + str(self.client_id).encode()
        
        for i in range(operations):
            operation_type = i % 10  # 10% PING, 45% PUT, 45% GET
            
            start_time = time.perf_counter()
            success = False
            
            try:
                if operation_type == 0:  # PING (10%)
                    self.socket.send(ping_cmd)
                    response = self.socket.recv(1)
                    success = len(response) == 1 and response[0] == RESP_PONG
                
                elif operation_type < 5:  # PUT (40%)
                    key = f"{key_template}{i}".encode()
                    value = value_template + str(i).encode()
                    
                    # Build PUT command
                    command = bytearray()
                    command.append(CMD_PUT)
                    command.extend(struct.pack('<I', len(key)))
                    command.extend(key)
                    command.extend(struct.pack('<I', len(value)))
                    command.extend(value)
                    command.append(0)  # No TTL
                    
                    self.socket.send(command)
                    response = self.socket.recv(1)
                    success = len(response) == 1 and response[0] == RESP_OK
                
                else:  # GET (50%)
                    # Get a previously stored key
                    key_id = max(0, i - 10)  # Get recent keys
                    key = f"{key_template}{key_id}".encode()
                    
                    # Build GET command
                    command = bytearray()
                    command.append(CMD_GET)
                    command.extend(struct.pack('<I', len(key)))
                    command.extend(key)
                    
                    self.socket.send(command)
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
                
                end_time = time.perf_counter()
                latency_ms = (end_time - start_time) * 1000
                
                if success:
                    results["successful_operations"] += 1
                    results["latencies"].append(latency_ms)
                else:
                    results["failed_operations"] += 1
                    
            except Exception:
                results["failed_operations"] += 1
        
        return results

class HighConcurrencyBenchmark:
    """High concurrency benchmark for maximum throughput"""
    
    def __init__(self, config: HighConcurrencyConfig):
        self.config = config
    
    def run_high_concurrency_benchmark(self):
        """Run high concurrency benchmark"""
        print("🚀 CrabCache HIGH CONCURRENCY Performance Benchmark")
        print("=" * 60)
        print(f"Server: {self.config.host}:{self.config.port}")
        print(f"Connections: {self.config.connections}")
        print(f"Operations per connection: {self.config.operations_per_connection}")
        print(f"Total operations: {self.config.connections * self.config.operations_per_connection:,}")
        print()
        
        # Test connection
        if not self.test_connection():
            print("❌ Connection test failed")
            return
        
        # Run benchmark
        print("🔧 Running high concurrency benchmark...")
        start_time = time.perf_counter()
        
        results = self.run_concurrent_workers()
        
        end_time = time.perf_counter()
        duration = end_time - start_time
        
        # Calculate metrics
        total_operations = results["successful_operations"] + results["failed_operations"]
        throughput = results["successful_operations"] / duration if duration > 0 else 0
        success_rate = results["successful_operations"] / total_operations * 100 if total_operations > 0 else 0
        
        # Calculate latencies
        if results["latencies"]:
            latency_p50 = statistics.median(results["latencies"])
            latency_p95 = statistics.quantiles(results["latencies"], n=20)[18] if len(results["latencies"]) > 20 else max(results["latencies"])
            latency_p99 = statistics.quantiles(results["latencies"], n=100)[98] if len(results["latencies"]) > 100 else max(results["latencies"])
        else:
            latency_p50 = latency_p95 = latency_p99 = 0
        
        # Print results
        self.print_high_concurrency_results({
            "total_operations": total_operations,
            "successful_operations": results["successful_operations"],
            "failed_operations": results["failed_operations"],
            "duration_seconds": duration,
            "throughput_ops_per_sec": throughput,
            "success_rate": success_rate,
            "latency_p50_ms": latency_p50,
            "latency_p95_ms": latency_p95,
            "latency_p99_ms": latency_p99,
        })
        
        # Save results
        self.save_results({
            "config": {
                "connections": self.config.connections,
                "operations_per_connection": self.config.operations_per_connection,
            },
            "results": {
                "total_operations": total_operations,
                "successful_operations": results["successful_operations"],
                "throughput_ops_per_sec": throughput,
                "success_rate": success_rate,
                "latency_p50_ms": latency_p50,
                "latency_p95_ms": latency_p95,
                "latency_p99_ms": latency_p99,
                "duration_seconds": duration,
            }
        })
    
    def test_connection(self) -> bool:
        """Test single connection"""
        print("🔧 Testing connection...")
        client = OptimizedClient(self.config.host, self.config.port, 0)
        if client.connect():
            client.disconnect()
            print("  ✅ Connection test passed")
            return True
        else:
            print("  ❌ Connection test failed")
            return False
    
    def run_concurrent_workers(self) -> dict:
        """Run concurrent workers for maximum throughput"""
        combined_results = {
            "successful_operations": 0,
            "failed_operations": 0,
            "latencies": []
        }
        
        def worker(client_id: int):
            client = OptimizedClient(self.config.host, self.config.port, client_id)
            
            if not client.connect():
                return {
                    "successful_operations": 0,
                    "failed_operations": self.config.operations_per_connection,
                    "latencies": []
                }
            
            try:
                results = client.run_high_throughput_operations(self.config.operations_per_connection)
                client.disconnect()
                return results
            except Exception:
                client.disconnect()
                return {
                    "successful_operations": 0,
                    "failed_operations": self.config.operations_per_connection,
                    "latencies": []
                }
        
        # Run workers with high concurrency
        with ThreadPoolExecutor(max_workers=self.config.connections) as executor:
            futures = [executor.submit(worker, i) for i in range(self.config.connections)]
            
            for future in futures:
                worker_results = future.result()
                combined_results["successful_operations"] += worker_results["successful_operations"]
                combined_results["failed_operations"] += worker_results["failed_operations"]
                combined_results["latencies"].extend(worker_results["latencies"])
        
        return combined_results
    
    def print_high_concurrency_results(self, result: dict):
        """Print high concurrency results"""
        print("📊 HIGH CONCURRENCY Benchmark Results:")
        print("=" * 50)
        print(f"Total Operations: {result['total_operations']:,}")
        print(f"Successful Operations: {result['successful_operations']:,}")
        print(f"Failed Operations: {result['failed_operations']:,}")
        print(f"Duration: {result['duration_seconds']:.2f}s")
        print(f"Throughput: {result['throughput_ops_per_sec']:,.0f} ops/sec")
        print(f"Success Rate: {result['success_rate']:.1f}%")
        print()
        
        print("📈 Latency Metrics:")
        print(f"  P50: {result['latency_p50_ms']:.2f}ms")
        print(f"  P95: {result['latency_p95_ms']:.2f}ms")
        print(f"  P99: {result['latency_p99_ms']:.2f}ms")
        print()
        
        # Performance comparison
        throughput = result['throughput_ops_per_sec']
        
        # Baselines
        phase2_baseline = 5092
        phase3_initial = 21588
        redis_baseline = 37498
        
        print("🥊 Performance Comparison:")
        if throughput > phase2_baseline:
            improvement = (throughput / phase2_baseline - 1) * 100
            print(f"  vs Phase 2: +{improvement:.1f}% ({throughput:,.0f} vs {phase2_baseline:,})")
        
        if throughput > phase3_initial:
            improvement = (throughput / phase3_initial - 1) * 100
            print(f"  vs Phase 3 Initial: +{improvement:.1f}% ({throughput:,.0f} vs {phase3_initial:,})")
        
        redis_ratio = throughput / redis_baseline * 100
        print(f"  vs Redis: {redis_ratio:.1f}% ({throughput:,.0f} vs {redis_baseline:,})")
        
        # Goals assessment
        if throughput > redis_baseline:
            print(f"  🏆 REDIS SURPASSED! (+{throughput - redis_baseline:,.0f} ops/sec)")
        elif throughput > 40000:
            print(f"  🎯 STRETCH GOAL ACHIEVED! ({throughput:,.0f} >= 40,000)")
        elif throughput > 20000:
            print(f"  ✅ MINIMUM GOAL ACHIEVED! ({throughput:,.0f} >= 20,000)")
            remaining = redis_baseline - throughput
            print(f"  🥊 Redis gap: {remaining:,.0f} ops/sec remaining")
        else:
            remaining = 20000 - throughput
            print(f"  ❌ Goal not reached. Need {remaining:,.0f} more ops/sec")
        
        # Validation
        if result['success_rate'] < 95:
            print(f"\n⚠️  WARNING: Low success rate ({result['success_rate']:.1f}%)")
    
    def save_results(self, data: dict):
        """Save results to file"""
        timestamp = time.strftime("%Y%m%d_%H%M%S")
        filename = f"crabcache/benchmark_results/high_concurrency_results_{timestamp}.json"
        
        with open(filename, 'w') as f:
            json.dump(data, f, indent=2)
        
        print(f"\n💾 Results saved to: {filename}")

def main():
    # Test different concurrency levels
    configs = [
        HighConcurrencyConfig(connections=20, operations_per_connection=2000),
        HighConcurrencyConfig(connections=50, operations_per_connection=1000),
        HighConcurrencyConfig(connections=100, operations_per_connection=500),
    ]
    
    best_throughput = 0
    best_config = None
    
    for i, config in enumerate(configs):
        print(f"\n🔧 Test {i+1}/3: {config.connections} connections, {config.operations_per_connection} ops each")
        print("-" * 60)
        
        benchmark = HighConcurrencyBenchmark(config)
        
        # Capture throughput (simplified - would need to modify to return it)
        benchmark.run_high_concurrency_benchmark()
        
        print("\n" + "="*60)
    
    print(f"\n🏆 High Concurrency Testing Complete!")
    print("Check the saved results for detailed comparison.")

if __name__ == "__main__":
    main()