#!/usr/bin/env python3
"""
Realistic Performance Benchmark for CrabCache Phase 3

This script provides ACCURATE and REALISTIC performance measurements
by ensuring operations actually succeed and measuring real throughput.
"""

import socket
import struct
import time
import statistics
import threading
from concurrent.futures import ThreadPoolExecutor
from dataclasses import dataclass
from typing import List, Tuple

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
class RealisticConfig:
    host: str = "127.0.0.1"
    port: int = 7001
    connections: int = 10
    operations_per_connection: int = 1000
    test_duration: int = 30

class ReliableClient:
    """Reliable client that ensures operations actually work"""
    
    def __init__(self, host: str, port: int):
        self.host = host
        self.port = port
        self.socket = None
        self.connected = False
    
    def connect(self) -> bool:
        """Connect and verify it works"""
        try:
            self.socket = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
            self.socket.setsockopt(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)
            self.socket.settimeout(10.0)  # 10 second timeout
            self.socket.connect((self.host, self.port))
            
            # Test with a PING to verify connection works
            self.socket.send(bytes([CMD_PING]))
            response = self.socket.recv(1)
            if len(response) == 1 and response[0] == RESP_PONG:
                self.connected = True
                return True
            else:
                self.disconnect()
                return False
        except Exception as e:
            print(f"Connection failed: {e}")
            return False
    
    def disconnect(self):
        """Disconnect from server"""
        if self.socket:
            try:
                self.socket.close()
            except:
                pass
            self.socket = None
            self.connected = False
    
    def ping(self) -> bool:
        """Send PING and verify PONG response"""
        if not self.connected:
            return False
        
        try:
            # Send PING
            self.socket.send(bytes([CMD_PING]))
            
            # Receive PONG
            response = self.socket.recv(1)
            return len(response) == 1 and response[0] == RESP_PONG
        except Exception:
            return False
    
    def put(self, key: bytes, value: bytes) -> bool:
        """Send PUT and verify OK response"""
        if not self.connected:
            return False
        
        try:
            # Build PUT command
            command = bytearray()
            command.append(CMD_PUT)
            command.extend(struct.pack('<I', len(key)))
            command.extend(key)
            command.extend(struct.pack('<I', len(value)))
            command.extend(value)
            command.append(0)  # No TTL
            
            # Send command
            self.socket.send(command)
            
            # Receive response
            response = self.socket.recv(1)
            return len(response) == 1 and response[0] == RESP_OK
        except Exception:
            return False
    
    def get(self, key: bytes) -> Tuple[bool, bytes]:
        """Send GET and return success + value"""
        if not self.connected:
            return False, b""
        
        try:
            # Build GET command
            command = bytearray()
            command.append(CMD_GET)
            command.extend(struct.pack('<I', len(key)))
            command.extend(key)
            
            # Send command
            self.socket.send(command)
            
            # Receive response type
            response_type = self.socket.recv(1)
            if len(response_type) != 1:
                return False, b""
            
            if response_type[0] == RESP_NULL:
                return True, b""  # Key not found, but operation succeeded
            elif response_type[0] == RESP_VALUE:
                # Read value length
                len_bytes = self.socket.recv(4)
                if len(len_bytes) != 4:
                    return False, b""
                
                value_len = struct.unpack('<I', len_bytes)[0]
                
                # Read value
                value = b""
                while len(value) < value_len:
                    chunk = self.socket.recv(value_len - len(value))
                    if not chunk:
                        return False, b""
                    value += chunk
                
                return True, value
            else:
                return False, b""
        except Exception:
            return False, b""

class RealisticBenchmark:
    """Realistic benchmark with accurate measurements"""
    
    def __init__(self, config: RealisticConfig):
        self.config = config
    
    def run_benchmark(self):
        """Run realistic benchmark"""
        print("🔍 CrabCache REALISTIC Performance Benchmark")
        print("=" * 50)
        print(f"Server: {self.config.host}:{self.config.port}")
        print(f"Connections: {self.config.connections}")
        print(f"Operations per connection: {self.config.operations_per_connection}")
        print()
        
        # Test single connection first
        if not self.test_single_connection():
            print("❌ Single connection test failed - server may not be working")
            return
        
        # Run benchmarks
        benchmarks = [
            ("PING Performance", self.benchmark_ping),
            ("PUT Performance", self.benchmark_put),
            ("GET Performance", self.benchmark_get),
            ("Mixed Workload", self.benchmark_mixed),
        ]
        
        results = {}
        for name, benchmark_func in benchmarks:
            print(f"🔧 Running {name}...")
            result = benchmark_func()
            results[name] = result
            self.print_result(name, result)
            print()
        
        # Summary
        self.print_summary(results)
    
    def test_single_connection(self) -> bool:
        """Test that a single connection works"""
        print("🔧 Testing single connection...")
        
        client = ReliableClient(self.config.host, self.config.port)
        if not client.connect():
            return False
        
        # Test basic operations
        if not client.ping():
            print("  ❌ PING failed")
            client.disconnect()
            return False
        
        if not client.put(b"test_key", b"test_value"):
            print("  ❌ PUT failed")
            client.disconnect()
            return False
        
        success, value = client.get(b"test_key")
        if not success:
            print("  ❌ GET failed")
            client.disconnect()
            return False
        
        client.disconnect()
        print("  ✅ Single connection test passed")
        return True
    
    def benchmark_ping(self) -> dict:
        """Benchmark PING operations"""
        return self.run_worker_benchmark(lambda client: client.ping(), "PING")
    
    def benchmark_put(self) -> dict:
        """Benchmark PUT operations"""
        def put_operation(client):
            key = f"bench_key_{time.time_ns()}".encode()[:16]
            value = b"x" * 64
            return client.put(key, value)
        
        return self.run_worker_benchmark(put_operation, "PUT")
    
    def benchmark_get(self) -> dict:
        """Benchmark GET operations"""
        # Pre-populate some data
        self.populate_test_data(100)
        
        def get_operation(client):
            key_id = hash(time.time_ns()) % 100
            key = f"test_key_{key_id:03d}".encode()
            success, _ = client.get(key)
            return success
        
        return self.run_worker_benchmark(get_operation, "GET")
    
    def benchmark_mixed(self) -> dict:
        """Benchmark mixed workload"""
        self.populate_test_data(50)
        
        def mixed_operation(client):
            rand = hash(time.time_ns()) % 100
            if rand < 70:  # 70% GET
                key_id = hash(time.time_ns()) % 50
                key = f"test_key_{key_id:03d}".encode()
                success, _ = client.get(key)
                return success
            elif rand < 90:  # 20% PUT
                key = f"mixed_key_{time.time_ns()}".encode()[:16]
                value = b"mixed_value"
                return client.put(key, value)
            else:  # 10% PING
                return client.ping()
        
        return self.run_worker_benchmark(mixed_operation, "MIXED")
    
    def run_worker_benchmark(self, operation_func, operation_name: str) -> dict:
        """Run benchmark with multiple workers"""
        latencies = []
        errors = 0
        successful_operations = 0
        
        def worker():
            nonlocal errors, successful_operations
            worker_latencies = []
            worker_errors = 0
            worker_successes = 0
            
            # Create and test connection
            client = ReliableClient(self.config.host, self.config.port)
            if not client.connect():
                worker_errors += self.config.operations_per_connection
                return worker_latencies, worker_errors, worker_successes
            
            try:
                for _ in range(self.config.operations_per_connection):
                    start_time = time.perf_counter()
                    success = operation_func(client)
                    end_time = time.perf_counter()
                    
                    latency_ms = (end_time - start_time) * 1000
                    worker_latencies.append(latency_ms)
                    
                    if success:
                        worker_successes += 1
                    else:
                        worker_errors += 1
                
                client.disconnect()
            except Exception as e:
                print(f"Worker error: {e}")
                worker_errors += self.config.operations_per_connection - worker_successes
                client.disconnect()
            
            return worker_latencies, worker_errors, worker_successes
        
        # Run workers
        print(f"  Running {self.config.connections} workers...")
        start_time = time.perf_counter()
        
        with ThreadPoolExecutor(max_workers=self.config.connections) as executor:
            futures = [executor.submit(worker) for _ in range(self.config.connections)]
            
            for future in futures:
                worker_latencies, worker_errors, worker_successes = future.result()
                latencies.extend(worker_latencies)
                errors += worker_errors
                successful_operations += worker_successes
        
        end_time = time.perf_counter()
        duration = end_time - start_time
        
        # Calculate metrics
        total_operations = successful_operations + errors
        throughput = successful_operations / duration if duration > 0 else 0
        success_rate = successful_operations / total_operations * 100 if total_operations > 0 else 0
        
        if latencies:
            latency_p50 = statistics.median(latencies)
            latency_p95 = statistics.quantiles(latencies, n=20)[18] if len(latencies) > 20 else max(latencies)
            latency_p99 = statistics.quantiles(latencies, n=100)[98] if len(latencies) > 100 else max(latencies)
        else:
            latency_p50 = latency_p95 = latency_p99 = 0
        
        return {
            "total_operations": total_operations,
            "successful_operations": successful_operations,
            "duration_seconds": duration,
            "throughput_ops_per_sec": throughput,
            "latency_p50_ms": latency_p50,
            "latency_p95_ms": latency_p95,
            "latency_p99_ms": latency_p99,
            "success_rate": success_rate,
            "errors": errors,
        }
    
    def populate_test_data(self, count: int):
        """Populate test data"""
        print(f"  📊 Populating {count} test entries...")
        
        client = ReliableClient(self.config.host, self.config.port)
        if not client.connect():
            print("  ❌ Failed to connect for data population")
            return
        
        for i in range(count):
            key = f"test_key_{i:03d}".encode()
            value = f"test_value_{i}".encode()
            client.put(key, value)
        
        client.disconnect()
    
    def print_result(self, name: str, result: dict):
        """Print benchmark result"""
        print(f"  📊 {name} Results:")
        print(f"    Total Operations: {result['total_operations']:,}")
        print(f"    Successful Operations: {result['successful_operations']:,}")
        print(f"    Duration: {result['duration_seconds']:.2f}s")
        print(f"    Throughput: {result['throughput_ops_per_sec']:,.0f} ops/sec")
        print(f"    Success Rate: {result['success_rate']:.1f}%")
        print(f"    Latency P50: {result['latency_p50_ms']:.2f}ms")
        print(f"    Latency P95: {result['latency_p95_ms']:.2f}ms")
        print(f"    Latency P99: {result['latency_p99_ms']:.2f}ms")
        print(f"    Errors: {result['errors']}")
        
        # Compare with known baselines
        throughput = result['throughput_ops_per_sec']
        
        # Phase 2 baseline
        phase2_baseline = 5092
        if throughput > phase2_baseline:
            improvement = (throughput / phase2_baseline - 1) * 100
            print(f"    🚀 {improvement:.1f}% improvement over Phase 2")
        else:
            degradation = (1 - throughput / phase2_baseline) * 100
            print(f"    📉 {degradation:.1f}% slower than Phase 2")
        
        # Phase 3 initial baseline
        phase3_baseline = 21588
        if throughput > phase3_baseline:
            improvement = (throughput / phase3_baseline - 1) * 100
            print(f"    🔥 {improvement:.1f}% improvement over Phase 3 initial")
        
        # Redis comparison
        redis_baseline = 37498
        redis_ratio = throughput / redis_baseline * 100
        print(f"    🥊 {redis_ratio:.1f}% of Redis performance")
        
        if throughput > redis_baseline:
            print(f"    🏆 SURPASSED REDIS! (+{redis_ratio-100:.1f}%)")
        elif throughput > 20000:
            print(f"    ✅ Above minimum goal (20,000 ops/sec)")
        
        # Validate results
        if result['success_rate'] < 95:
            print(f"    ⚠️  LOW SUCCESS RATE - Results may be unreliable")
        if result['throughput_ops_per_sec'] > 1000000:
            print(f"    ⚠️  SUSPICIOUSLY HIGH THROUGHPUT - Check for measurement errors")
    
    def print_summary(self, results: dict):
        """Print benchmark summary"""
        print("🎯 REALISTIC Benchmark Summary")
        print("=" * 50)
        
        # Find best throughput
        best_throughput = 0
        best_test = ""
        
        for test_name, result in results.items():
            if result['throughput_ops_per_sec'] > best_throughput:
                best_throughput = result['throughput_ops_per_sec']
                best_test = test_name
        
        print(f"Best Performance: {best_test}")
        print(f"Peak Throughput: {best_throughput:,.0f} ops/sec")
        
        # Validate overall results
        all_success_rates = [r['success_rate'] for r in results.values()]
        avg_success_rate = sum(all_success_rates) / len(all_success_rates)
        
        print(f"Average Success Rate: {avg_success_rate:.1f}%")
        
        if avg_success_rate < 95:
            print("⚠️  WARNING: Low success rates indicate connection or server issues")
        
        if best_throughput > 100000:
            print("⚠️  WARNING: Very high throughput - verify measurements are accurate")
        
        # Goals assessment
        redis_baseline = 37498
        if best_throughput > redis_baseline:
            print(f"🏆 REDIS SURPASSED! CrabCache is {best_throughput/redis_baseline:.1f}x faster!")
        elif best_throughput > 20000:
            print(f"✅ Minimum goal achieved ({best_throughput:,.0f} >= 20,000 ops/sec)")
        else:
            remaining = 20000 - best_throughput
            print(f"❌ Goal not reached. Need {remaining:,.0f} more ops/sec")

def main():
    config = RealisticConfig(
        host="127.0.0.1",
        port=7001,
        connections=10,
        operations_per_connection=1000
    )
    
    benchmark = RealisticBenchmark(config)
    benchmark.run_benchmark()

if __name__ == "__main__":
    main()