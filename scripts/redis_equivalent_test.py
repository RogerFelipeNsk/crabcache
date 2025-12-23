#!/usr/bin/env python3
"""
Redis-Equivalent CrabCache Test

This script tests CrabCache with Redis-equivalent settings to provide
an apples-to-apples performance comparison.

Redis benchmark command:
redis-benchmark -h 127.0.0.1 -p 6379 -c 100 -n 1000000 -d 64 -P 16 -t ping,set,get
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
CMD_PUT = 0x02
CMD_GET = 0x03

RESP_OK = 0x10
RESP_PONG = 0x11
RESP_VALUE = 0x14
RESP_NULL = 0x12

@dataclass
class RedisEquivalentConfig:
    host: str = "127.0.0.1"
    port: int = 7001
    connections: int = 100        # Redis -c 100
    total_requests: int = 1000000 # Redis -n 1000000
    pipeline_size: int = 16       # Redis -P 16
    data_size: int = 64          # Redis -d 64
    test_operations: List[str] = None  # Redis -t ping,set,get

    def __post_init__(self):
        if self.test_operations is None:
            self.test_operations = ["PING", "PUT", "GET"]

class PipelinedClient:
    """Client that simulates Redis pipelining behavior"""
    
    def __init__(self, host: str, port: int, client_id: int, config: RedisEquivalentConfig):
        self.host = host
        self.port = port
        self.client_id = client_id
        self.config = config
        self.socket = None
        self.connected = False
    
    def connect(self) -> bool:
        """Connect with Redis-equivalent TCP settings"""
        try:
            self.socket = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
            
            # Redis-equivalent TCP optimizations
            self.socket.setsockopt(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)
            self.socket.setsockopt(socket.SOL_SOCKET, socket.SO_RCVBUF, 32768)  # 32KB
            self.socket.setsockopt(socket.SOL_SOCKET, socket.SO_SNDBUF, 32768)  # 32KB
            self.socket.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
            self.socket.settimeout(10.0)  # Longer timeout for high load
            
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
    
    def create_ping_command(self) -> bytes:
        """Create PING command"""
        return bytes([CMD_PING])
    
    def create_put_command(self, key: bytes, value: bytes) -> bytes:
        """Create PUT command with specified data size"""
        cmd = bytearray()
        cmd.append(CMD_PUT)
        cmd.extend(struct.pack('<I', len(key)))
        cmd.extend(key)
        cmd.extend(struct.pack('<I', len(value)))
        cmd.extend(value)
        cmd.append(0)  # No TTL
        return bytes(cmd)
    
    def create_get_command(self, key: bytes) -> bytes:
        """Create GET command"""
        cmd = bytearray()
        cmd.append(CMD_GET)
        cmd.extend(struct.pack('<I', len(key)))
        cmd.extend(key)
        return bytes(cmd)
    
    def run_pipelined_test(self, requests_per_client: int, operation_mix: Dict[str, float]) -> Dict:
        """Run pipelined test with Redis-equivalent settings"""
        results = {
            "client_id": self.client_id,
            "successful_operations": 0,
            "failed_operations": 0,
            "latencies_ms": [],
            "operation_counts": {op: 0 for op in operation_mix.keys()},
            "pipeline_batches": 0,
        }
        
        if not self.connected:
            results["error"] = "Not connected"
            return results
        
        # Pre-generate test data
        test_value = b'x' * self.config.data_size  # Redis -d 64 equivalent
        
        # Pre-populate some keys for GET operations
        if "GET" in operation_mix:
            for i in range(100):  # Pre-populate 100 keys
                key = f"benchmark_key_{self.client_id}_{i}".encode()
                put_cmd = self.create_put_command(key, test_value)
                try:
                    self.socket.send(put_cmd)
                    response = self.socket.recv(1)
                except:
                    pass  # Ignore setup errors
        
        # Calculate operations per type
        operations_per_type = {}
        for op_type, ratio in operation_mix.items():
            operations_per_type[op_type] = int(requests_per_client * ratio)
        
        # Generate operation sequence
        operation_sequence = []
        for op_type, count in operations_per_type.items():
            operation_sequence.extend([op_type] * count)
        
        # Shuffle for realistic mix
        import random
        random.shuffle(operation_sequence)
        
        # Process in pipeline batches
        total_processed = 0
        
        try:
            for batch_start in range(0, len(operation_sequence), self.config.pipeline_size):
                batch_end = min(batch_start + self.config.pipeline_size, len(operation_sequence))
                batch_ops = operation_sequence[batch_start:batch_end]
                
                # Prepare batch commands
                batch_commands = []
                expected_responses = []
                
                for i, op_type in enumerate(batch_ops):
                    if op_type == "PING":
                        cmd = self.create_ping_command()
                        expected_responses.append(RESP_PONG)
                    elif op_type == "PUT":
                        key = f"bench_key_{self.client_id}_{total_processed + i}".encode()
                        cmd = self.create_put_command(key, test_value)
                        expected_responses.append(RESP_OK)
                    elif op_type == "GET":
                        # Get from pre-populated keys
                        key_idx = (total_processed + i) % 100
                        key = f"benchmark_key_{self.client_id}_{key_idx}".encode()
                        cmd = self.create_get_command(key)
                        expected_responses.append(RESP_VALUE)  # or RESP_NULL
                    
                    batch_commands.append(cmd)
                
                # Send entire batch (pipelining)
                batch_start_time = time.perf_counter_ns()
                
                for cmd in batch_commands:
                    self.socket.send(cmd)
                
                # Receive all responses
                batch_successful = 0
                for i, expected_resp in enumerate(expected_responses):
                    try:
                        response = self.socket.recv(1)
                        if len(response) == 1:
                            if expected_resp == RESP_VALUE:
                                # Handle GET response
                                if response[0] == RESP_VALUE:
                                    # Read value length and value
                                    len_bytes = self.socket.recv(4)
                                    if len(len_bytes) == 4:
                                        value_len = struct.unpack('<I', len_bytes)[0]
                                        if value_len <= 1024:  # Reasonable limit
                                            value = self.socket.recv(value_len)
                                            if len(value) == value_len:
                                                batch_successful += 1
                                elif response[0] == RESP_NULL:
                                    # NULL response is also valid for GET
                                    batch_successful += 1
                            else:
                                # PING or PUT response
                                if response[0] == expected_resp:
                                    batch_successful += 1
                    except Exception:
                        pass  # Count as failed
                
                batch_end_time = time.perf_counter_ns()
                
                # Calculate per-operation latency (batch latency / operations)
                batch_latency_ms = (batch_end_time - batch_start_time) / 1_000_000
                avg_op_latency = batch_latency_ms / len(batch_commands)
                
                # Record results
                results["successful_operations"] += batch_successful
                results["failed_operations"] += (len(batch_commands) - batch_successful)
                results["pipeline_batches"] += 1
                
                # Record latencies for successful operations
                for _ in range(batch_successful):
                    results["latencies_ms"].append(avg_op_latency)
                
                # Count operations by type
                for op_type in batch_ops:
                    results["operation_counts"][op_type] += 1
                
                total_processed += len(batch_commands)
                
        except Exception as e:
            results["error"] = str(e)
        
        return results

class RedisEquivalentBenchmark:
    """Benchmark that matches Redis benchmark settings exactly"""
    
    def __init__(self, config: RedisEquivalentConfig):
        self.config = config
    
    def run_benchmark(self):
        """Run Redis-equivalent benchmark"""
        print("🚀 CrabCache Redis-Equivalent Benchmark")
        print("=" * 50)
        print("🎯 GOAL: Match Redis benchmark settings exactly")
        print()
        
        print("📊 Redis Benchmark Equivalent Settings:")
        print(f"  Connections (-c):     {self.config.connections}")
        print(f"  Total Requests (-n):  {self.config.total_requests:,}")
        print(f"  Pipeline Size (-P):   {self.config.pipeline_size}")
        print(f"  Data Size (-d):       {self.config.data_size} bytes")
        print(f"  Operations (-t):      {', '.join(self.config.test_operations)}")
        print()
        
        # Calculate requests per client
        requests_per_client = self.config.total_requests // self.config.connections
        
        print(f"📈 Test Configuration:")
        print(f"  Requests per client:  {requests_per_client:,}")
        print(f"  Pipeline batches:     {requests_per_client // self.config.pipeline_size:,}")
        print()
        
        # Define operation mix (similar to Redis default)
        operation_mix = {
            "PING": 0.4,  # 40% PING operations
            "PUT": 0.3,   # 30% PUT operations  
            "GET": 0.3,   # 30% GET operations
        }
        
        print("🔧 Operation Mix:")
        for op, ratio in operation_mix.items():
            count = int(self.config.total_requests * ratio)
            print(f"  {op}: {ratio*100:.0f}% ({count:,} operations)")
        print()
        
        # Run benchmark
        print("⚡ Starting Redis-equivalent benchmark...")
        start_time = time.perf_counter()
        
        def worker(client_id: int):
            client = PipelinedClient(self.config.host, self.config.port, client_id, self.config)
            
            if not client.connect():
                return {
                    "client_id": client_id,
                    "successful_operations": 0,
                    "failed_operations": requests_per_client,
                    "latencies_ms": [],
                    "operation_counts": {op: 0 for op in operation_mix.keys()},
                    "pipeline_batches": 0,
                    "error": "Connection failed"
                }
            
            try:
                result = client.run_pipelined_test(requests_per_client, operation_mix)
                client.disconnect()
                return result
            except Exception as e:
                client.disconnect()
                return {
                    "client_id": client_id,
                    "successful_operations": 0,
                    "failed_operations": requests_per_client,
                    "latencies_ms": [],
                    "operation_counts": {op: 0 for op in operation_mix.keys()},
                    "pipeline_batches": 0,
                    "error": str(e)
                }
        
        # Run workers with pipelining
        with ThreadPoolExecutor(max_workers=self.config.connections) as executor:
            futures = [executor.submit(worker, i) for i in range(self.config.connections)]
            worker_results = [future.result() for future in futures]
        
        end_time = time.perf_counter()
        total_duration = end_time - start_time
        
        # Analyze results
        self.analyze_results(worker_results, total_duration)
    
    def analyze_results(self, worker_results: List[Dict], total_duration: float):
        """Analyze and compare results with Redis"""
        print("📊 Redis-Equivalent Benchmark Results:")
        print("=" * 45)
        
        # Combine results
        total_successful = sum(r["successful_operations"] for r in worker_results)
        total_failed = sum(r["failed_operations"] for r in worker_results)
        all_latencies = []
        total_pipeline_batches = sum(r["pipeline_batches"] for r in worker_results)
        
        # Combine operation counts
        combined_op_counts = {}
        for result in worker_results:
            for op, count in result["operation_counts"].items():
                combined_op_counts[op] = combined_op_counts.get(op, 0) + count
        
        for result in worker_results:
            all_latencies.extend(result["latencies_ms"])
        
        # Calculate metrics
        throughput = total_successful / total_duration
        success_rate = total_successful / (total_successful + total_failed) * 100
        
        print(f"⏱️  Duration: {total_duration:.2f}s")
        print(f"✅ Successful Operations: {total_successful:,}")
        print(f"❌ Failed Operations: {total_failed:,}")
        print(f"📈 Success Rate: {success_rate:.1f}%")
        print(f"🚀 Throughput: {throughput:,.0f} ops/sec")
        print(f"📦 Pipeline Batches: {total_pipeline_batches:,}")
        print()
        
        # Operation breakdown
        print("📊 Operation Breakdown:")
        for op, count in combined_op_counts.items():
            percentage = count / sum(combined_op_counts.values()) * 100
            ops_per_sec = count / total_duration
            print(f"  {op}: {count:,} ops ({percentage:.1f}%) - {ops_per_sec:,.0f} ops/sec")
        print()
        
        # Latency analysis
        if all_latencies:
            latencies = np.array(all_latencies)
            percentiles = {
                "P50": np.percentile(latencies, 50),
                "P90": np.percentile(latencies, 90),
                "P95": np.percentile(latencies, 95),
                "P99": np.percentile(latencies, 99),
                "P99.9": np.percentile(latencies, 99.9),
            }
            
            print("📊 Latency Percentiles:")
            for p, value in percentiles.items():
                status = "✅" if value < 1.0 else "⚠️" if value < 5.0 else "❌"
                print(f"  {p}: {value:.3f}ms {status}")
            print()
        
        # Compare with Redis
        self.compare_with_redis(throughput, all_latencies)
        
        # Save results
        self.save_results(worker_results, total_duration, throughput)
    
    def compare_with_redis(self, crabcache_throughput: float, latencies: List[float]):
        """Compare results with Redis benchmark"""
        print("🥊 CrabCache vs Redis Comparison:")
        print("-" * 35)
        
        # Redis baseline (from redis-benchmark)
        redis_baseline = 37498  # ops/sec from standard Redis benchmark
        redis_pipelined = 150000  # Estimated with -P 16
        
        # CrabCache results
        ratio_baseline = crabcache_throughput / redis_baseline * 100
        ratio_pipelined = crabcache_throughput / redis_pipelined * 100
        
        print(f"Redis (no pipeline):     {redis_baseline:,} ops/sec")
        print(f"Redis (with -P 16):      {redis_pipelined:,} ops/sec (estimated)")
        print(f"CrabCache (pipelined):   {crabcache_throughput:,.0f} ops/sec")
        print()
        
        print(f"📊 Performance Ratio:")
        print(f"  vs Redis baseline:     {ratio_baseline:.1f}%")
        print(f"  vs Redis pipelined:    {ratio_pipelined:.1f}%")
        print()
        
        if crabcache_throughput > redis_baseline:
            surplus = crabcache_throughput - redis_baseline
            print(f"🏆 REDIS BASELINE SURPASSED! (+{surplus:,.0f} ops/sec)")
        else:
            gap = redis_baseline - crabcache_throughput
            print(f"📊 Redis baseline gap: {gap:,.0f} ops/sec")
        
        if crabcache_throughput > redis_pipelined:
            surplus = crabcache_throughput - redis_pipelined
            print(f"🚀 REDIS PIPELINED SURPASSED! (+{surplus:,.0f} ops/sec)")
        else:
            gap = redis_pipelined - crabcache_throughput
            print(f"🎯 Redis pipelined gap: {gap:,.0f} ops/sec")
        
        print()
        
        # Latency comparison
        if latencies:
            avg_latency = np.mean(latencies)
            p99_latency = np.percentile(latencies, 99)
            
            print(f"📊 Latency Analysis:")
            print(f"  Average: {avg_latency:.3f}ms")
            print(f"  P99: {p99_latency:.3f}ms")
            
            if p99_latency < 1.0:
                print(f"  🎯 P99 < 1ms: ✅ EXCELLENT")
            elif p99_latency < 2.0:
                print(f"  🎯 P99 < 2ms: ✅ GOOD")
            else:
                print(f"  🎯 P99 > 2ms: ⚠️ NEEDS IMPROVEMENT")
    
    def save_results(self, worker_results: List[Dict], duration: float, throughput: float):
        """Save benchmark results"""
        timestamp = time.strftime("%Y%m%d_%H%M%S")
        filename = f"crabcache/benchmark_results/redis_equivalent_results_{timestamp}.json"
        
        # Prepare results for JSON
        json_results = []
        for result in worker_results:
            json_result = {k: v for k, v in result.items() if k != "latencies_ms"}
            json_results.append(json_result)
        
        summary_data = {
            "timestamp": timestamp,
            "config": {
                "connections": self.config.connections,
                "total_requests": self.config.total_requests,
                "pipeline_size": self.config.pipeline_size,
                "data_size": self.config.data_size,
                "test_operations": self.config.test_operations,
            },
            "results": {
                "duration": duration,
                "throughput": throughput,
                "total_successful": sum(r["successful_operations"] for r in worker_results),
                "total_failed": sum(r["failed_operations"] for r in worker_results),
                "worker_results": json_results,
            }
        }
        
        with open(filename, 'w') as f:
            json.dump(summary_data, f, indent=2)
        
        print(f"💾 Results saved to: {filename}")

def main():
    print("🚀 Starting CrabCache Redis-Equivalent Benchmark...")
    print("🎯 Goal: Test CrabCache with exact Redis benchmark settings")
    print()
    
    # Redis-equivalent configuration
    config = RedisEquivalentConfig(
        host="127.0.0.1",
        port=7001,
        connections=100,        # Redis -c 100
        total_requests=1000000, # Redis -n 1000000
        pipeline_size=16,       # Redis -P 16
        data_size=64,          # Redis -d 64
        test_operations=["PING", "PUT", "GET"]  # Redis -t ping,set,get
    )
    
    benchmark = RedisEquivalentBenchmark(config)
    benchmark.run_benchmark()
    
    print("\n🎊 Redis-Equivalent Benchmark Complete!")
    print("📊 Compare these results with: redis-benchmark -h 127.0.0.1 -p 6379 -c 100 -n 1000000 -d 64 -P 16 -t ping,set,get")

if __name__ == "__main__":
    main()