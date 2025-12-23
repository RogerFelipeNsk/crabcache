#!/usr/bin/env python3
"""
Phase 3 Performance Benchmark for CrabCache

This script tests the Phase 3 improvements including:
- Native client with binary protocol
- Connection pooling
- Pipelining
- SIMD operations (indirectly)
- Zero-copy operations (indirectly)

Target: 20,000+ ops/sec (surpass Redis baseline of 37,498 ops/sec)
"""

import asyncio
import socket
import struct
import time
import statistics
import json
from concurrent.futures import ThreadPoolExecutor
from dataclasses import dataclass
from typing import List, Dict, Any
import argparse

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
class BenchmarkConfig:
    host: str = "127.0.0.1"
    port: int = 7001
    connections: int = 50
    operations: int = 100000
    key_size: int = 16
    value_size: int = 64
    pipeline_size: int = 100
    test_duration: int = 60
    warmup_duration: int = 10

@dataclass
class BenchmarkResult:
    total_operations: int
    duration_seconds: float
    throughput_ops_per_sec: float
    latency_p50_ms: float
    latency_p95_ms: float
    latency_p99_ms: float
    success_rate: float
    errors: int
    connections_used: int

class BinaryProtocolClient:
    """High-performance binary protocol client"""
    
    def __init__(self, host: str, port: int):
        self.host = host
        self.port = port
        self.socket = None
    
    async def connect(self):
        """Connect to CrabCache server"""
        self.socket = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        self.socket.setsockopt(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)
        self.socket.connect((self.host, self.port))
    
    def disconnect(self):
        """Disconnect from server"""
        if self.socket:
            self.socket.close()
            self.socket = None
    
    def ping(self) -> bool:
        """Send PING command"""
        try:
            # Send PING (1 byte)
            self.socket.send(bytes([CMD_PING]))
            
            # Receive PONG (1 byte)
            response = self.socket.recv(1)
            return len(response) == 1 and response[0] == RESP_PONG
        except Exception:
            return False
    
    def put(self, key: bytes, value: bytes) -> bool:
        """Send PUT command"""
        try:
            # Build PUT command: [CMD_PUT][key_len][key][value_len][value][ttl_flag]
            command = bytearray()
            command.append(CMD_PUT)
            command.extend(struct.pack('<I', len(key)))
            command.extend(key)
            command.extend(struct.pack('<I', len(value)))
            command.extend(value)
            command.append(0)  # No TTL
            
            self.socket.send(command)
            
            # Receive OK (1 byte)
            response = self.socket.recv(1)
            return len(response) == 1 and response[0] == RESP_OK
        except Exception:
            return False
    
    def get(self, key: bytes) -> tuple[bool, bytes]:
        """Send GET command"""
        try:
            # Build GET command: [CMD_GET][key_len][key]
            command = bytearray()
            command.append(CMD_GET)
            command.extend(struct.pack('<I', len(key)))
            command.extend(key)
            
            self.socket.send(command)
            
            # Receive response type
            response_type = self.socket.recv(1)
            if len(response_type) != 1:
                return False, b""
            
            if response_type[0] == RESP_NULL:
                return True, b""
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
    
    def pipeline_operations(self, operations: List[tuple]) -> List[bool]:
        """Execute multiple operations in pipeline"""
        try:
            # Send all commands
            for op_type, *args in operations:
                if op_type == "ping":
                    self.socket.send(bytes([CMD_PING]))
                elif op_type == "put":
                    key, value = args
                    command = bytearray()
                    command.append(CMD_PUT)
                    command.extend(struct.pack('<I', len(key)))
                    command.extend(key)
                    command.extend(struct.pack('<I', len(value)))
                    command.extend(value)
                    command.append(0)  # No TTL
                    self.socket.send(command)
                elif op_type == "get":
                    key = args[0]
                    command = bytearray()
                    command.append(CMD_GET)
                    command.extend(struct.pack('<I', len(key)))
                    command.extend(key)
                    self.socket.send(command)
            
            # Receive all responses
            results = []
            for op_type, *args in operations:
                if op_type == "ping":
                    response = self.socket.recv(1)
                    results.append(len(response) == 1 and response[0] == RESP_PONG)
                elif op_type == "put":
                    response = self.socket.recv(1)
                    results.append(len(response) == 1 and response[0] == RESP_OK)
                elif op_type == "get":
                    response_type = self.socket.recv(1)
                    if len(response_type) == 1:
                        if response_type[0] == RESP_NULL:
                            results.append(True)
                        elif response_type[0] == RESP_VALUE:
                            len_bytes = self.socket.recv(4)
                            if len(len_bytes) == 4:
                                value_len = struct.unpack('<I', len_bytes)[0]
                                value = self.socket.recv(value_len)
                                results.append(len(value) == value_len)
                            else:
                                results.append(False)
                        else:
                            results.append(False)
                    else:
                        results.append(False)
            
            return results
        except Exception:
            return [False] * len(operations)

class Phase3Benchmark:
    """Phase 3 performance benchmark suite"""
    
    def __init__(self, config: BenchmarkConfig):
        self.config = config
        self.results = {}
    
    async def run_all_benchmarks(self):
        """Run all Phase 3 benchmarks"""
        print("🚀 Phase 3 Performance Benchmark Suite")
        print("=" * 50)
        print(f"Target: 20,000+ ops/sec (Redis baseline: 37,498 ops/sec)")
        print(f"Server: {self.config.host}:{self.config.port}")
        print(f"Connections: {self.config.connections}")
        print(f"Operations: {self.config.operations}")
        print()
        
        # Test connection
        if not await self.test_connection():
            print("❌ Cannot connect to CrabCache server")
            return
        
        # Run benchmarks
        benchmarks = [
            ("PING Throughput", self.benchmark_ping),
            ("PUT Throughput", self.benchmark_put),
            ("GET Throughput", self.benchmark_get),
            ("Mixed Workload", self.benchmark_mixed),
            ("Pipeline Performance", self.benchmark_pipeline),
            ("High Concurrency", self.benchmark_high_concurrency),
        ]
        
        for name, benchmark_func in benchmarks:
            print(f"🔧 Running {name}...")
            result = await benchmark_func()
            self.results[name] = result
            self.print_result(name, result)
            print()
        
        # Summary
        self.print_summary()
    
    async def test_connection(self) -> bool:
        """Test connection to server"""
        try:
            client = BinaryProtocolClient(self.config.host, self.config.port)
            client.socket = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
            client.socket.setsockopt(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)
            client.socket.connect((client.host, client.port))
            success = client.ping()
            client.disconnect()
            return success
        except Exception as e:
            print(f"Connection test failed: {e}")
            return False
    
    async def benchmark_ping(self) -> BenchmarkResult:
        """Benchmark PING operations"""
        return await self.run_benchmark_worker(
            lambda client: client.ping(),
            "PING"
        )
    
    async def benchmark_put(self) -> BenchmarkResult:
        """Benchmark PUT operations"""
        def put_operation(client):
            key = f"bench_key_{time.time_ns()}".encode()[:self.config.key_size]
            value = b"x" * self.config.value_size
            return client.put(key, value)
        
        return await self.run_benchmark_worker(put_operation, "PUT")
    
    async def benchmark_get(self) -> BenchmarkResult:
        """Benchmark GET operations"""
        # Pre-populate some keys
        await self.populate_test_data(1000)
        
        def get_operation(client):
            key_id = hash(time.time_ns()) % 1000
            key = f"test_key_{key_id:04d}".encode()
            success, _ = client.get(key)
            return success
        
        return await self.run_benchmark_worker(get_operation, "GET")
    
    async def benchmark_mixed(self) -> BenchmarkResult:
        """Benchmark mixed workload (70% GET, 20% PUT, 10% PING)"""
        await self.populate_test_data(1000)
        
        def mixed_operation(client):
            rand = hash(time.time_ns()) % 100
            if rand < 70:  # 70% GET
                key_id = hash(time.time_ns()) % 1000
                key = f"test_key_{key_id:04d}".encode()
                success, _ = client.get(key)
                return success
            elif rand < 90:  # 20% PUT
                key = f"bench_key_{time.time_ns()}".encode()[:self.config.key_size]
                value = b"x" * self.config.value_size
                return client.put(key, value)
            else:  # 10% PING
                return client.ping()
        
        return await self.run_benchmark_worker(mixed_operation, "MIXED")
    
    async def benchmark_pipeline(self) -> BenchmarkResult:
        """Benchmark pipeline operations"""
        def pipeline_operation(client):
            operations = []
            for i in range(self.config.pipeline_size):
                if i % 3 == 0:
                    operations.append(("ping",))
                elif i % 3 == 1:
                    key = f"pipe_key_{i}".encode()
                    value = f"pipe_value_{i}".encode()
                    operations.append(("put", key, value))
                else:
                    key = f"pipe_key_{i-1}".encode()
                    operations.append(("get", key))
            
            results = client.pipeline_operations(operations)
            return all(results)
        
        # Adjust operations count for pipeline
        original_ops = self.config.operations
        self.config.operations = self.config.operations // self.config.pipeline_size
        
        result = await self.run_benchmark_worker(pipeline_operation, "PIPELINE")
        
        # Adjust result to account for pipeline multiplier
        result.total_operations *= self.config.pipeline_size
        result.throughput_ops_per_sec *= self.config.pipeline_size
        
        self.config.operations = original_ops
        return result
    
    async def benchmark_high_concurrency(self) -> BenchmarkResult:
        """Benchmark with high concurrency"""
        original_connections = self.config.connections
        self.config.connections = min(100, self.config.connections * 2)
        
        result = await self.benchmark_mixed()
        
        self.config.connections = original_connections
        return result
    
    async def run_benchmark_worker(self, operation_func, operation_name: str) -> BenchmarkResult:
        """Run benchmark with multiple workers"""
        operations_per_worker = self.config.operations // self.config.connections
        latencies = []
        errors = 0
        successful_operations = 0
        
        def worker():
            nonlocal errors, successful_operations
            worker_latencies = []
            worker_errors = 0
            worker_successes = 0
            
            try:
                client = BinaryProtocolClient(self.config.host, self.config.port)
                client.socket = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
                client.socket.setsockopt(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)
                client.socket.connect((client.host, client.port))
                
                for _ in range(operations_per_worker):
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
                worker_errors += operations_per_worker
                print(f"Worker error: {e}")
            
            return worker_latencies, worker_errors, worker_successes
        
        # Run workers
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
        throughput = total_operations / duration if duration > 0 else 0
        success_rate = successful_operations / total_operations * 100 if total_operations > 0 else 0
        
        if latencies:
            latency_p50 = statistics.median(latencies)
            latency_p95 = statistics.quantiles(latencies, n=20)[18]  # 95th percentile
            latency_p99 = statistics.quantiles(latencies, n=100)[98]  # 99th percentile
        else:
            latency_p50 = latency_p95 = latency_p99 = 0
        
        return BenchmarkResult(
            total_operations=total_operations,
            duration_seconds=duration,
            throughput_ops_per_sec=throughput,
            latency_p50_ms=latency_p50,
            latency_p95_ms=latency_p95,
            latency_p99_ms=latency_p99,
            success_rate=success_rate,
            errors=errors,
            connections_used=self.config.connections
        )
    
    async def populate_test_data(self, count: int):
        """Populate test data for GET benchmarks"""
        client = BinaryProtocolClient(self.config.host, self.config.port)
        client.socket = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        client.socket.setsockopt(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)
        client.socket.connect((client.host, client.port))
        
        for i in range(count):
            key = f"test_key_{i:04d}".encode()
            value = f"test_value_{i}".encode()
            client.put(key, value)
        
        client.disconnect()
    
    def print_result(self, name: str, result: BenchmarkResult):
        """Print benchmark result"""
        print(f"  📊 {name} Results:")
        print(f"    Operations: {result.total_operations:,}")
        print(f"    Duration: {result.duration_seconds:.2f}s")
        print(f"    Throughput: {result.throughput_ops_per_sec:,.0f} ops/sec")
        print(f"    Success Rate: {result.success_rate:.1f}%")
        print(f"    Latency P50: {result.latency_p50_ms:.2f}ms")
        print(f"    Latency P95: {result.latency_p95_ms:.2f}ms")
        print(f"    Latency P99: {result.latency_p99_ms:.2f}ms")
        print(f"    Errors: {result.errors}")
        
        # Compare with Phase 2 baseline
        phase2_baseline = 5092
        if result.throughput_ops_per_sec > phase2_baseline:
            improvement = (result.throughput_ops_per_sec / phase2_baseline - 1) * 100
            print(f"    🚀 {improvement:.1f}% improvement over Phase 2!")
        else:
            degradation = (1 - result.throughput_ops_per_sec / phase2_baseline) * 100
            print(f"    📉 {degradation:.1f}% slower than Phase 2")
        
        # Compare with Redis baseline
        redis_baseline = 37498
        redis_ratio = result.throughput_ops_per_sec / redis_baseline * 100
        print(f"    🥊 {redis_ratio:.1f}% of Redis performance")
        
        if result.throughput_ops_per_sec > redis_baseline:
            print(f"    🏆 SURPASSED REDIS! (+{redis_ratio-100:.1f}%)")
    
    def print_summary(self):
        """Print benchmark summary"""
        print("🎯 Phase 3 Benchmark Summary")
        print("=" * 50)
        
        # Find best throughput
        best_throughput = 0
        best_test = ""
        
        for test_name, result in self.results.items():
            if result.throughput_ops_per_sec > best_throughput:
                best_throughput = result.throughput_ops_per_sec
                best_test = test_name
        
        print(f"Best Performance: {best_test}")
        print(f"Peak Throughput: {best_throughput:,.0f} ops/sec")
        
        # Phase comparison
        phase2_baseline = 5092
        redis_baseline = 37498
        
        if best_throughput > phase2_baseline:
            phase_improvement = (best_throughput / phase2_baseline - 1) * 100
            print(f"Phase 2 Improvement: +{phase_improvement:.1f}%")
        
        redis_ratio = best_throughput / redis_baseline * 100
        print(f"Redis Comparison: {redis_ratio:.1f}%")
        
        # Goals assessment
        print("\n🎯 Phase 3 Goals Assessment:")
        target_min = 20000
        target_stretch = 40000
        
        if best_throughput >= target_stretch:
            print(f"✅ STRETCH GOAL ACHIEVED! ({best_throughput:,.0f} >= {target_stretch:,})")
        elif best_throughput >= target_min:
            print(f"✅ MINIMUM GOAL ACHIEVED! ({best_throughput:,.0f} >= {target_min:,})")
        else:
            remaining = target_min - best_throughput
            print(f"❌ Goal not reached. Need {remaining:,.0f} more ops/sec")
        
        if best_throughput > redis_baseline:
            print("🏆 REDIS SURPASSED! Mission accomplished!")
        else:
            remaining = redis_baseline - best_throughput
            print(f"🥊 Redis gap: {remaining:,.0f} ops/sec remaining")
        
        # Save results
        self.save_results()
    
    def save_results(self):
        """Save benchmark results to file"""
        timestamp = time.strftime("%Y%m%d_%H%M%S")
        filename = f"phase3_benchmark_results_{timestamp}.json"
        
        results_data = {
            "timestamp": timestamp,
            "config": {
                "host": self.config.host,
                "port": self.config.port,
                "connections": self.config.connections,
                "operations": self.config.operations,
            },
            "results": {}
        }
        
        for test_name, result in self.results.items():
            results_data["results"][test_name] = {
                "total_operations": result.total_operations,
                "duration_seconds": result.duration_seconds,
                "throughput_ops_per_sec": result.throughput_ops_per_sec,
                "latency_p50_ms": result.latency_p50_ms,
                "latency_p95_ms": result.latency_p95_ms,
                "latency_p99_ms": result.latency_p99_ms,
                "success_rate": result.success_rate,
                "errors": result.errors,
            }
        
        with open(filename, 'w') as f:
            json.dump(results_data, f, indent=2)
        
        print(f"\n💾 Results saved to: {filename}")

async def main():
    parser = argparse.ArgumentParser(description="Phase 3 Performance Benchmark")
    parser.add_argument("--host", default="127.0.0.1", help="CrabCache host")
    parser.add_argument("--port", type=int, default=7001, help="CrabCache port")
    parser.add_argument("--connections", type=int, default=50, help="Number of connections")
    parser.add_argument("--operations", type=int, default=100000, help="Total operations")
    parser.add_argument("--pipeline-size", type=int, default=100, help="Pipeline batch size")
    
    args = parser.parse_args()
    
    config = BenchmarkConfig(
        host=args.host,
        port=args.port,
        connections=args.connections,
        operations=args.operations,
        pipeline_size=args.pipeline_size
    )
    
    benchmark = Phase3Benchmark(config)
    await benchmark.run_all_benchmarks()

if __name__ == "__main__":
    asyncio.run(main())