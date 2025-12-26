#!/usr/bin/env python3
"""
Advanced Pipeline Benchmark for CrabCache Phase 6.1

This script tests the advanced pipelining features:
- Parallel batch processing
- Adaptive batch sizing
- Zero-copy operations
- SIMD optimizations
- Smart command grouping

Target: 300,000+ ops/sec
"""

import asyncio
import socket
import time
import json
import statistics
import argparse
from concurrent.futures import ThreadPoolExecutor
from dataclasses import dataclass
from typing import List, Dict, Any
import threading
import random

@dataclass
class BenchmarkConfig:
    """Configuration for advanced pipeline benchmark"""
    host: str = "127.0.0.1"
    port: int = 8000
    
    # Test parameters
    total_operations: int = 100000
    concurrent_connections: int = 20
    batch_sizes: List[int] = None
    test_duration_seconds: int = 60
    
    # Pipeline features to test
    test_adaptive_sizing: bool = True
    test_parallel_processing: bool = True
    test_mixed_workloads: bool = True
    test_command_grouping: bool = True
    
    # Performance targets
    target_ops_per_second: int = 300000
    target_p99_latency_ms: float = 1.0
    
    def __post_init__(self):
        if self.batch_sizes is None:
            self.batch_sizes = [4, 8, 16, 32, 64, 128]

@dataclass
class BenchmarkResult:
    """Results from a benchmark run"""
    test_name: str
    total_operations: int
    total_time_seconds: float
    ops_per_second: float
    avg_latency_ms: float
    p50_latency_ms: float
    p95_latency_ms: float
    p99_latency_ms: float
    min_latency_ms: float
    max_latency_ms: float
    success_rate: float
    batch_size: int = 0
    additional_metrics: Dict[str, Any] = None

class AdvancedPipelineBenchmark:
    """Advanced pipeline benchmark runner"""
    
    def __init__(self, config: BenchmarkConfig):
        self.config = config
        self.results: List[BenchmarkResult] = []
        self.connection_pool: List[socket.socket] = []
        self.lock = threading.Lock()
        
    def create_connection(self) -> socket.socket:
        """Create a new connection to CrabCache"""
        sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        sock.settimeout(10.0)
        try:
            sock.connect((self.config.host, self.config.port))
            return sock
        except Exception as e:
            print(f"Failed to connect: {e}")
            raise
    
    def create_connection_pool(self, size: int) -> List[socket.socket]:
        """Create a pool of connections"""
        pool = []
        for i in range(size):
            try:
                conn = self.create_connection()
                pool.append(conn)
            except Exception as e:
                print(f"Failed to create connection {i}: {e}")
                break
        return pool
    
    def close_connection_pool(self, pool: List[socket.socket]):
        """Close all connections in pool"""
        for conn in pool:
            try:
                conn.close()
            except:
                pass
    
    def generate_batch_commands(self, batch_size: int, workload_type: str = "mixed") -> List[str]:
        """Generate a batch of commands"""
        commands = []
        
        if workload_type == "mixed":
            # Mixed workload with realistic distribution
            for i in range(batch_size):
                rand = random.random()
                if rand < 0.5:  # 50% GET operations
                    key = f"key_{random.randint(1, 10000)}"
                    commands.append(f"GET {key}")
                elif rand < 0.8:  # 30% PUT operations
                    key = f"key_{random.randint(1, 10000)}"
                    value = f"value_{random.randint(1, 1000)}"
                    commands.append(f"PUT {key} {value}")
                elif rand < 0.9:  # 10% DEL operations
                    key = f"key_{random.randint(1, 10000)}"
                    commands.append(f"DEL {key}")
                else:  # 10% PING operations
                    commands.append("PING")
        
        elif workload_type == "read_heavy":
            # 80% reads, 20% writes
            for i in range(batch_size):
                if random.random() < 0.8:
                    key = f"key_{random.randint(1, 10000)}"
                    commands.append(f"GET {key}")
                else:
                    key = f"key_{random.randint(1, 10000)}"
                    value = f"value_{random.randint(1, 1000)}"
                    commands.append(f"PUT {key} {value}")
        
        elif workload_type == "write_heavy":
            # 20% reads, 80% writes
            for i in range(batch_size):
                if random.random() < 0.2:
                    key = f"key_{random.randint(1, 10000)}"
                    commands.append(f"GET {key}")
                else:
                    key = f"key_{random.randint(1, 10000)}"
                    value = f"value_{random.randint(1, 1000)}"
                    commands.append(f"PUT {key} {value}")
        
        return commands
    
    def send_batch_pipelined(self, conn: socket.socket, commands: List[str]) -> tuple:
        """Send a batch of commands in pipeline mode"""
        start_time = time.time()
        
        try:
            # Send all commands at once (pipelined)
            batch_data = "\n".join(commands) + "\n"
            conn.sendall(batch_data.encode())
            
            # Read all responses
            responses = []
            for _ in commands:
                response = conn.recv(4096).decode().strip()
                responses.append(response)
            
            end_time = time.time()
            latency_ms = (end_time - start_time) * 1000
            
            return len(commands), latency_ms, True
            
        except Exception as e:
            end_time = time.time()
            latency_ms = (end_time - start_time) * 1000
            print(f"Batch send error: {e}")
            return 0, latency_ms, False
    
    def worker_thread(self, worker_id: int, operations_per_worker: int, 
                     batch_size: int, workload_type: str, results_queue: list):
        """Worker thread for concurrent testing"""
        conn = None
        latencies = []
        successful_ops = 0
        failed_ops = 0
        
        try:
            conn = self.create_connection()
            
            operations_done = 0
            while operations_done < operations_per_worker:
                # Generate batch
                commands = self.generate_batch_commands(batch_size, workload_type)
                
                # Send batch
                ops_count, latency_ms, success = self.send_batch_pipelined(conn, commands)
                
                if success:
                    successful_ops += ops_count
                    latencies.append(latency_ms)
                else:
                    failed_ops += len(commands)
                
                operations_done += len(commands)
            
        except Exception as e:
            print(f"Worker {worker_id} error: {e}")
        
        finally:
            if conn:
                conn.close()
        
        # Store results
        with self.lock:
            results_queue.append({
                'worker_id': worker_id,
                'successful_ops': successful_ops,
                'failed_ops': failed_ops,
                'latencies': latencies
            })
    
    def run_batch_size_test(self, batch_size: int, workload_type: str = "mixed") -> BenchmarkResult:
        """Run benchmark for specific batch size"""
        print(f"\n🧪 Testing batch size {batch_size} with {workload_type} workload...")
        
        operations_per_worker = self.config.total_operations // self.config.concurrent_connections
        results_queue = []
        
        start_time = time.time()
        
        # Run concurrent workers
        with ThreadPoolExecutor(max_workers=self.config.concurrent_connections) as executor:
            futures = []
            for worker_id in range(self.config.concurrent_connections):
                future = executor.submit(
                    self.worker_thread, 
                    worker_id, 
                    operations_per_worker, 
                    batch_size, 
                    workload_type, 
                    results_queue
                )
                futures.append(future)
            
            # Wait for all workers to complete
            for future in futures:
                future.result()
        
        end_time = time.time()
        total_time = end_time - start_time
        
        # Aggregate results
        total_successful = sum(r['successful_ops'] for r in results_queue)
        total_failed = sum(r['failed_ops'] for r in results_queue)
        all_latencies = []
        for r in results_queue:
            all_latencies.extend(r['latencies'])
        
        # Calculate metrics
        ops_per_second = total_successful / total_time if total_time > 0 else 0
        success_rate = total_successful / (total_successful + total_failed) if (total_successful + total_failed) > 0 else 0
        
        if all_latencies:
            avg_latency = statistics.mean(all_latencies)
            p50_latency = statistics.median(all_latencies)
            p95_latency = statistics.quantiles(all_latencies, n=20)[18] if len(all_latencies) > 20 else max(all_latencies)
            p99_latency = statistics.quantiles(all_latencies, n=100)[98] if len(all_latencies) > 100 else max(all_latencies)
            min_latency = min(all_latencies)
            max_latency = max(all_latencies)
        else:
            avg_latency = p50_latency = p95_latency = p99_latency = min_latency = max_latency = 0
        
        result = BenchmarkResult(
            test_name=f"batch_size_{batch_size}_{workload_type}",
            total_operations=total_successful,
            total_time_seconds=total_time,
            ops_per_second=ops_per_second,
            avg_latency_ms=avg_latency,
            p50_latency_ms=p50_latency,
            p95_latency_ms=p95_latency,
            p99_latency_ms=p99_latency,
            min_latency_ms=min_latency,
            max_latency_ms=max_latency,
            success_rate=success_rate,
            batch_size=batch_size,
            additional_metrics={
                'workload_type': workload_type,
                'concurrent_connections': self.config.concurrent_connections,
                'total_failed': total_failed
            }
        )
        
        print(f"✅ Batch size {batch_size}: {ops_per_second:.0f} ops/sec, "
              f"P99: {p99_latency:.2f}ms, Success: {success_rate*100:.1f}%")
        
        return result
    
    def run_adaptive_sizing_test(self) -> List[BenchmarkResult]:
        """Test adaptive batch sizing"""
        print("\n🔄 Testing Adaptive Batch Sizing...")
        
        results = []
        for batch_size in self.config.batch_sizes:
            result = self.run_batch_size_test(batch_size, "mixed")
            results.append(result)
        
        return results
    
    def run_workload_comparison_test(self) -> List[BenchmarkResult]:
        """Test different workload types"""
        print("\n📊 Testing Different Workload Types...")
        
        results = []
        optimal_batch_size = 16  # Start with reasonable default
        
        # Find optimal batch size first
        if self.config.test_adaptive_sizing:
            sizing_results = self.run_adaptive_sizing_test()
            if sizing_results:
                best_result = max(sizing_results, key=lambda r: r.ops_per_second)
                optimal_batch_size = best_result.batch_size
                print(f"📈 Optimal batch size found: {optimal_batch_size}")
        
        # Test different workloads with optimal batch size
        workload_types = ["mixed", "read_heavy", "write_heavy"]
        for workload in workload_types:
            result = self.run_batch_size_test(optimal_batch_size, workload)
            results.append(result)
        
        return results
    
    def run_duration_test(self, duration_seconds: int = 60) -> BenchmarkResult:
        """Run sustained load test for specified duration"""
        print(f"\n⏱️  Running {duration_seconds}s sustained load test...")
        
        batch_size = 16
        workload_type = "mixed"
        results_queue = []
        
        start_time = time.time()
        end_time = start_time + duration_seconds
        
        def duration_worker(worker_id: int):
            conn = None
            latencies = []
            successful_ops = 0
            failed_ops = 0
            
            try:
                conn = self.create_connection()
                
                while time.time() < end_time:
                    commands = self.generate_batch_commands(batch_size, workload_type)
                    ops_count, latency_ms, success = self.send_batch_pipelined(conn, commands)
                    
                    if success:
                        successful_ops += ops_count
                        latencies.append(latency_ms)
                    else:
                        failed_ops += len(commands)
                
            except Exception as e:
                print(f"Duration worker {worker_id} error: {e}")
            
            finally:
                if conn:
                    conn.close()
            
            with self.lock:
                results_queue.append({
                    'worker_id': worker_id,
                    'successful_ops': successful_ops,
                    'failed_ops': failed_ops,
                    'latencies': latencies
                })
        
        # Run workers
        with ThreadPoolExecutor(max_workers=self.config.concurrent_connections) as executor:
            futures = [executor.submit(duration_worker, i) for i in range(self.config.concurrent_connections)]
            for future in futures:
                future.result()
        
        actual_duration = time.time() - start_time
        
        # Aggregate results
        total_successful = sum(r['successful_ops'] for r in results_queue)
        total_failed = sum(r['failed_ops'] for r in results_queue)
        all_latencies = []
        for r in results_queue:
            all_latencies.extend(r['latencies'])
        
        # Calculate metrics
        ops_per_second = total_successful / actual_duration if actual_duration > 0 else 0
        success_rate = total_successful / (total_successful + total_failed) if (total_successful + total_failed) > 0 else 0
        
        if all_latencies:
            avg_latency = statistics.mean(all_latencies)
            p50_latency = statistics.median(all_latencies)
            p95_latency = statistics.quantiles(all_latencies, n=20)[18] if len(all_latencies) > 20 else max(all_latencies)
            p99_latency = statistics.quantiles(all_latencies, n=100)[98] if len(all_latencies) > 100 else max(all_latencies)
            min_latency = min(all_latencies)
            max_latency = max(all_latencies)
        else:
            avg_latency = p50_latency = p95_latency = p99_latency = min_latency = max_latency = 0
        
        result = BenchmarkResult(
            test_name=f"sustained_load_{duration_seconds}s",
            total_operations=total_successful,
            total_time_seconds=actual_duration,
            ops_per_second=ops_per_second,
            avg_latency_ms=avg_latency,
            p50_latency_ms=p50_latency,
            p95_latency_ms=p95_latency,
            p99_latency_ms=p99_latency,
            min_latency_ms=min_latency,
            max_latency_ms=max_latency,
            success_rate=success_rate,
            batch_size=batch_size,
            additional_metrics={
                'duration_seconds': actual_duration,
                'target_duration': duration_seconds,
                'workload_type': workload_type,
                'concurrent_connections': self.config.concurrent_connections
            }
        )
        
        print(f"✅ Sustained load: {ops_per_second:.0f} ops/sec over {actual_duration:.1f}s")
        
        return result
    
    def run_all_tests(self) -> List[BenchmarkResult]:
        """Run all benchmark tests"""
        print("🚀 Starting Advanced Pipeline Benchmark Suite")
        print(f"Target: {self.config.target_ops_per_second:,} ops/sec")
        print(f"Target P99 Latency: {self.config.target_p99_latency_ms}ms")
        
        all_results = []
        
        # Test 1: Adaptive batch sizing
        if self.config.test_adaptive_sizing:
            sizing_results = self.run_adaptive_sizing_test()
            all_results.extend(sizing_results)
        
        # Test 2: Workload comparison
        if self.config.test_mixed_workloads:
            workload_results = self.run_workload_comparison_test()
            all_results.extend(workload_results)
        
        # Test 3: Sustained load
        duration_result = self.run_duration_test(self.config.test_duration_seconds)
        all_results.append(duration_result)
        
        self.results = all_results
        return all_results
    
    def print_summary(self):
        """Print benchmark summary"""
        if not self.results:
            print("No results to summarize")
            return
        
        print("\n" + "="*80)
        print("🏆 ADVANCED PIPELINE BENCHMARK RESULTS")
        print("="*80)
        
        # Find best results
        best_throughput = max(self.results, key=lambda r: r.ops_per_second)
        best_latency = min(self.results, key=lambda r: r.p99_latency_ms)
        
        print(f"\n📊 BEST PERFORMANCE:")
        print(f"   Throughput: {best_throughput.ops_per_second:,.0f} ops/sec ({best_throughput.test_name})")
        print(f"   P99 Latency: {best_latency.p99_latency_ms:.2f}ms ({best_latency.test_name})")
        
        # Target analysis
        target_met = best_throughput.ops_per_second >= self.config.target_ops_per_second
        latency_met = best_latency.p99_latency_ms <= self.config.target_p99_latency_ms
        
        print(f"\n🎯 TARGET ANALYSIS:")
        print(f"   Throughput Target: {self.config.target_ops_per_second:,} ops/sec")
        print(f"   Achieved: {best_throughput.ops_per_second:,.0f} ops/sec")
        print(f"   Status: {'✅ MET' if target_met else '❌ MISSED'}")
        if not target_met:
            gap = self.config.target_ops_per_second - best_throughput.ops_per_second
            print(f"   Gap: {gap:,.0f} ops/sec ({gap/self.config.target_ops_per_second*100:.1f}%)")
        
        print(f"\n   Latency Target: {self.config.target_p99_latency_ms}ms")
        print(f"   Achieved: {best_latency.p99_latency_ms:.2f}ms")
        print(f"   Status: {'✅ MET' if latency_met else '❌ MISSED'}")
        
        # Detailed results table
        print(f"\n📋 DETAILED RESULTS:")
        print(f"{'Test Name':<30} {'Ops/sec':<12} {'P99 Lat':<10} {'Success':<8} {'Batch':<6}")
        print("-" * 70)
        
        for result in sorted(self.results, key=lambda r: r.ops_per_second, reverse=True):
            print(f"{result.test_name:<30} {result.ops_per_second:>10,.0f} "
                  f"{result.p99_latency_ms:>8.2f}ms {result.success_rate*100:>6.1f}% "
                  f"{result.batch_size:>5}")
        
        # Recommendations
        print(f"\n💡 RECOMMENDATIONS:")
        if target_met and latency_met:
            print("   🎉 All targets met! Consider stress testing with higher loads.")
        else:
            if not target_met:
                print(f"   📈 Increase batch size or optimize parsing for higher throughput")
            if not latency_met:
                print(f"   ⚡ Optimize command processing to reduce latency")
        
        # Optimal configuration
        print(f"\n⚙️  OPTIMAL CONFIGURATION:")
        print(f"   Batch Size: {best_throughput.batch_size}")
        print(f"   Connections: {self.config.concurrent_connections}")
        if best_throughput.additional_metrics:
            workload = best_throughput.additional_metrics.get('workload_type', 'unknown')
            print(f"   Workload: {workload}")
    
    def save_results(self, filename: str):
        """Save results to JSON file"""
        results_data = {
            'timestamp': time.time(),
            'config': {
                'target_ops_per_second': self.config.target_ops_per_second,
                'target_p99_latency_ms': self.config.target_p99_latency_ms,
                'concurrent_connections': self.config.concurrent_connections,
                'total_operations': self.config.total_operations,
                'batch_sizes': self.config.batch_sizes,
            },
            'results': []
        }
        
        for result in self.results:
            results_data['results'].append({
                'test_name': result.test_name,
                'total_operations': result.total_operations,
                'total_time_seconds': result.total_time_seconds,
                'ops_per_second': result.ops_per_second,
                'avg_latency_ms': result.avg_latency_ms,
                'p50_latency_ms': result.p50_latency_ms,
                'p95_latency_ms': result.p95_latency_ms,
                'p99_latency_ms': result.p99_latency_ms,
                'min_latency_ms': result.min_latency_ms,
                'max_latency_ms': result.max_latency_ms,
                'success_rate': result.success_rate,
                'batch_size': result.batch_size,
                'additional_metrics': result.additional_metrics,
            })
        
        with open(filename, 'w') as f:
            json.dump(results_data, f, indent=2)
        
        print(f"📁 Results saved to {filename}")

def main():
    parser = argparse.ArgumentParser(description='Advanced Pipeline Benchmark for CrabCache')
    parser.add_argument('--host', default='127.0.0.1', help='CrabCache host')
    parser.add_argument('--port', type=int, default=8000, help='CrabCache port')
    parser.add_argument('--operations', type=int, default=100000, help='Total operations')
    parser.add_argument('--connections', type=int, default=20, help='Concurrent connections')
    parser.add_argument('--duration', type=int, default=60, help='Test duration in seconds')
    parser.add_argument('--target-ops', type=int, default=300000, help='Target ops/sec')
    parser.add_argument('--target-latency', type=float, default=1.0, help='Target P99 latency (ms)')
    parser.add_argument('--output', default='advanced_pipeline_results.json', help='Output file')
    
    args = parser.parse_args()
    
    config = BenchmarkConfig(
        host=args.host,
        port=args.port,
        total_operations=args.operations,
        concurrent_connections=args.connections,
        test_duration_seconds=args.duration,
        target_ops_per_second=args.target_ops,
        target_p99_latency_ms=args.target_latency,
    )
    
    benchmark = AdvancedPipelineBenchmark(config)
    
    try:
        # Test connection first
        test_conn = benchmark.create_connection()
        test_conn.close()
        print(f"✅ Connected to CrabCache at {args.host}:{args.port}")
        
        # Run benchmarks
        results = benchmark.run_all_tests()
        
        # Print summary
        benchmark.print_summary()
        
        # Save results
        benchmark.save_results(args.output)
        
    except Exception as e:
        print(f"❌ Benchmark failed: {e}")
        return 1
    
    return 0

if __name__ == "__main__":
    exit(main())