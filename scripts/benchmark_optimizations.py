#!/usr/bin/env python3
"""
Optimization Benchmark for CrabCache Phase 6.1

This script specifically tests the SIMD and zero-copy optimizations
to validate the 300,000+ ops/sec target performance.
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
class OptimizationBenchmarkConfig:
    """Configuration for optimization benchmark"""
    host: str = "127.0.0.1"
    port: int = 8000
    
    # Test parameters
    total_operations: int = 200000
    concurrent_connections: int = 32
    batch_sizes: List[int] = None
    test_duration_seconds: int = 30
    
    # Optimization tests
    test_simd_performance: bool = True
    test_zero_copy_performance: bool = True
    test_combined_optimizations: bool = True
    
    # Performance targets
    target_ops_per_second: int = 300000
    target_p99_latency_ms: float = 1.0
    
    def __post_init__(self):
        if self.batch_sizes is None:
            self.batch_sizes = [8, 16, 32, 64, 128, 256]

@dataclass
class OptimizationResult:
    """Results from optimization benchmark"""
    test_name: str
    optimization_type: str
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
    cpu_usage_percent: float = 0.0
    memory_usage_mb: float = 0.0
    additional_metrics: Dict[str, Any] = None

class OptimizationBenchmark:
    """Optimization benchmark runner"""
    
    def __init__(self, config: OptimizationBenchmarkConfig):
        self.config = config
        self.results: List[OptimizationResult] = []
        self.lock = threading.Lock()
        
    def create_connection(self) -> socket.socket:
        """Create a new connection to CrabCache"""
        sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        sock.settimeout(5.0)
        try:
            sock.connect((self.config.host, self.config.port))
            return sock
        except Exception as e:
            print(f"Failed to connect: {e}")
            raise
    
    def generate_optimized_batch(self, batch_size: int, optimization_type: str = "mixed") -> List[str]:
        """Generate batch optimized for specific features"""
        commands = []
        
        if optimization_type == "simd_optimized":
            # Generate commands that benefit from SIMD parsing
            for i in range(batch_size):
                if i % 4 == 0:
                    commands.append("PING")
                elif i % 4 == 1:
                    commands.append(f"GET key_{i:06d}")
                elif i % 4 == 2:
                    commands.append(f"PUT key_{i:06d} value_{i:06d}")
                else:
                    commands.append(f"DEL key_{i:06d}")
        
        elif optimization_type == "zero_copy_optimized":
            # Generate commands that benefit from zero-copy operations
            for i in range(batch_size):
                if i % 3 == 0:
                    # Short keys for zero-copy efficiency
                    commands.append(f"GET k{i}")
                elif i % 3 == 1:
                    # Medium values
                    commands.append(f"PUT k{i} v{i}")
                else:
                    commands.append(f"DEL k{i}")
        
        elif optimization_type == "large_batch":
            # Large batch to test parallel processing
            for i in range(batch_size):
                rand = random.random()
                if rand < 0.4:
                    commands.append(f"GET large_key_{i:08d}")
                elif rand < 0.7:
                    commands.append(f"PUT large_key_{i:08d} large_value_{i:08d}")
                elif rand < 0.9:
                    commands.append(f"DEL large_key_{i:08d}")
                else:
                    commands.append("PING")
        
        else:  # mixed
            for i in range(batch_size):
                rand = random.random()
                if rand < 0.5:
                    commands.append(f"GET key_{i}")
                elif rand < 0.8:
                    commands.append(f"PUT key_{i} value_{i}")
                elif rand < 0.95:
                    commands.append(f"DEL key_{i}")
                else:
                    commands.append("PING")
        
        return commands
    
    def send_optimized_batch(self, conn: socket.socket, commands: List[str]) -> tuple:
        """Send optimized batch and measure performance"""
        start_time = time.time()
        
        try:
            # Send all commands in pipeline
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
            print(f"Optimized batch send error: {e}")
            return 0, latency_ms, False
    
    def optimization_worker(self, worker_id: int, operations_per_worker: int, 
                          batch_size: int, optimization_type: str, results_queue: list):
        """Worker thread for optimization testing"""
        conn = None
        latencies = []
        successful_ops = 0
        failed_ops = 0
        
        try:
            conn = self.create_connection()
            
            operations_done = 0
            while operations_done < operations_per_worker:
                # Generate optimized batch
                commands = self.generate_optimized_batch(batch_size, optimization_type)
                
                # Send batch
                ops_count, latency_ms, success = self.send_optimized_batch(conn, commands)
                
                if success:
                    successful_ops += ops_count
                    latencies.append(latency_ms)
                else:
                    failed_ops += len(commands)
                
                operations_done += len(commands)
            
        except Exception as e:
            print(f"Optimization worker {worker_id} error: {e}")
        
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
    
    def run_optimization_test(self, optimization_type: str, batch_size: int) -> OptimizationResult:
        """Run optimization-specific test"""
        print(f"\n🔬 Testing {optimization_type} optimization (batch size: {batch_size})...")
        
        operations_per_worker = self.config.total_operations // self.config.concurrent_connections
        results_queue = []
        
        start_time = time.time()
        
        # Run concurrent workers
        with ThreadPoolExecutor(max_workers=self.config.concurrent_connections) as executor:
            futures = []
            for worker_id in range(self.config.concurrent_connections):
                future = executor.submit(
                    self.optimization_worker, 
                    worker_id, 
                    operations_per_worker, 
                    batch_size, 
                    optimization_type, 
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
        
        result = OptimizationResult(
            test_name=f"{optimization_type}_batch_{batch_size}",
            optimization_type=optimization_type,
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
                'concurrent_connections': self.config.concurrent_connections,
                'total_failed': total_failed
            }
        )
        
        print(f"✅ {optimization_type}: {ops_per_second:.0f} ops/sec, "
              f"P99: {p99_latency:.2f}ms, Success: {success_rate*100:.1f}%")
        
        return result
    
    def run_simd_performance_test(self) -> List[OptimizationResult]:
        """Test SIMD optimization performance"""
        print("\n🚀 Testing SIMD Performance Optimizations...")
        
        results = []
        for batch_size in [16, 32, 64, 128]:
            result = self.run_optimization_test("simd_optimized", batch_size)
            results.append(result)
        
        return results
    
    def run_zero_copy_performance_test(self) -> List[OptimizationResult]:
        """Test zero-copy optimization performance"""
        print("\n📦 Testing Zero-Copy Performance Optimizations...")
        
        results = []
        for batch_size in [32, 64, 128, 256]:
            result = self.run_optimization_test("zero_copy_optimized", batch_size)
            results.append(result)
        
        return results
    
    def run_combined_optimizations_test(self) -> List[OptimizationResult]:
        """Test combined optimizations"""
        print("\n⚡ Testing Combined Optimizations...")
        
        results = []
        for batch_size in [64, 128, 256, 512]:
            result = self.run_optimization_test("large_batch", batch_size)
            results.append(result)
        
        return results
    
    def run_sustained_performance_test(self, duration_seconds: int = 60) -> OptimizationResult:
        """Run sustained performance test"""
        print(f"\n⏱️  Running {duration_seconds}s sustained performance test...")
        
        batch_size = 128  # Optimal batch size
        optimization_type = "large_batch"
        results_queue = []
        
        start_time = time.time()
        end_time = start_time + duration_seconds
        
        def sustained_worker(worker_id: int):
            conn = None
            latencies = []
            successful_ops = 0
            failed_ops = 0
            
            try:
                conn = self.create_connection()
                
                while time.time() < end_time:
                    commands = self.generate_optimized_batch(batch_size, optimization_type)
                    ops_count, latency_ms, success = self.send_optimized_batch(conn, commands)
                    
                    if success:
                        successful_ops += ops_count
                        latencies.append(latency_ms)
                    else:
                        failed_ops += len(commands)
                
            except Exception as e:
                print(f"Sustained worker {worker_id} error: {e}")
            
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
            futures = [executor.submit(sustained_worker, i) for i in range(self.config.concurrent_connections)]
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
        
        result = OptimizationResult(
            test_name=f"sustained_performance_{duration_seconds}s",
            optimization_type="combined",
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
                'concurrent_connections': self.config.concurrent_connections
            }
        )
        
        print(f"✅ Sustained: {ops_per_second:.0f} ops/sec over {actual_duration:.1f}s")
        
        return result
    
    def run_all_optimization_tests(self) -> List[OptimizationResult]:
        """Run all optimization tests"""
        print("🚀 Starting Optimization Benchmark Suite")
        print(f"Target: {self.config.target_ops_per_second:,} ops/sec")
        print(f"Target P99 Latency: {self.config.target_p99_latency_ms}ms")
        
        all_results = []
        
        # Test 1: SIMD optimizations
        if self.config.test_simd_performance:
            simd_results = self.run_simd_performance_test()
            all_results.extend(simd_results)
        
        # Test 2: Zero-copy optimizations
        if self.config.test_zero_copy_performance:
            zero_copy_results = self.run_zero_copy_performance_test()
            all_results.extend(zero_copy_results)
        
        # Test 3: Combined optimizations
        if self.config.test_combined_optimizations:
            combined_results = self.run_combined_optimizations_test()
            all_results.extend(combined_results)
        
        # Test 4: Sustained performance
        sustained_result = self.run_sustained_performance_test(self.config.test_duration_seconds)
        all_results.append(sustained_result)
        
        self.results = all_results
        return all_results
    
    def print_optimization_summary(self):
        """Print optimization benchmark summary"""
        if not self.results:
            print("No results to summarize")
            return
        
        print("\n" + "="*80)
        print("🏆 OPTIMIZATION BENCHMARK RESULTS")
        print("="*80)
        
        # Find best results by optimization type
        simd_results = [r for r in self.results if r.optimization_type == "simd_optimized"]
        zero_copy_results = [r for r in self.results if r.optimization_type == "zero_copy_optimized"]
        combined_results = [r for r in self.results if r.optimization_type in ["large_batch", "combined"]]
        
        best_overall = max(self.results, key=lambda r: r.ops_per_second)
        best_latency = min(self.results, key=lambda r: r.p99_latency_ms)
        
        print(f"\n📊 BEST PERFORMANCE:")
        print(f"   Overall Throughput: {best_overall.ops_per_second:,.0f} ops/sec ({best_overall.test_name})")
        print(f"   Best P99 Latency: {best_latency.p99_latency_ms:.2f}ms ({best_latency.test_name})")
        
        # Optimization-specific results
        if simd_results:
            best_simd = max(simd_results, key=lambda r: r.ops_per_second)
            print(f"   SIMD Optimized: {best_simd.ops_per_second:,.0f} ops/sec (batch: {best_simd.batch_size})")
        
        if zero_copy_results:
            best_zero_copy = max(zero_copy_results, key=lambda r: r.ops_per_second)
            print(f"   Zero-Copy Optimized: {best_zero_copy.ops_per_second:,.0f} ops/sec (batch: {best_zero_copy.batch_size})")
        
        if combined_results:
            best_combined = max(combined_results, key=lambda r: r.ops_per_second)
            print(f"   Combined Optimizations: {best_combined.ops_per_second:,.0f} ops/sec (batch: {best_combined.batch_size})")
        
        # Target analysis
        target_met = best_overall.ops_per_second >= self.config.target_ops_per_second
        latency_met = best_latency.p99_latency_ms <= self.config.target_p99_latency_ms
        
        print(f"\n🎯 TARGET ANALYSIS:")
        print(f"   Throughput Target: {self.config.target_ops_per_second:,} ops/sec")
        print(f"   Achieved: {best_overall.ops_per_second:,.0f} ops/sec")
        print(f"   Status: {'✅ MET' if target_met else '❌ MISSED'}")
        if not target_met:
            gap = self.config.target_ops_per_second - best_overall.ops_per_second
            print(f"   Gap: {gap:,.0f} ops/sec ({gap/self.config.target_ops_per_second*100:.1f}%)")
        
        print(f"\n   Latency Target: {self.config.target_p99_latency_ms}ms")
        print(f"   Achieved: {best_latency.p99_latency_ms:.2f}ms")
        print(f"   Status: {'✅ MET' if latency_met else '❌ MISSED'}")
        
        # Detailed results table
        print(f"\n📋 DETAILED RESULTS:")
        print(f"{'Test Name':<35} {'Type':<15} {'Ops/sec':<12} {'P99 Lat':<10} {'Batch':<6}")
        print("-" * 80)
        
        for result in sorted(self.results, key=lambda r: r.ops_per_second, reverse=True):
            print(f"{result.test_name:<35} {result.optimization_type:<15} "
                  f"{result.ops_per_second:>10,.0f} {result.p99_latency_ms:>8.2f}ms "
                  f"{result.batch_size:>5}")
        
        # Performance improvement analysis
        baseline_ops = 219000  # From previous benchmarks
        improvement = (best_overall.ops_per_second - baseline_ops) / baseline_ops * 100
        
        print(f"\n📈 PERFORMANCE IMPROVEMENT:")
        print(f"   Baseline (Phase 3): {baseline_ops:,} ops/sec")
        print(f"   Phase 6.1 Optimized: {best_overall.ops_per_second:,.0f} ops/sec")
        print(f"   Improvement: {improvement:+.1f}%")
        
        if improvement >= 37:  # Target improvement
            print(f"   Status: 🎉 TARGET IMPROVEMENT EXCEEDED!")
        elif improvement >= 25:
            print(f"   Status: ✅ GOOD IMPROVEMENT")
        else:
            print(f"   Status: 🟡 NEEDS MORE OPTIMIZATION")
    
    def save_optimization_results(self, filename: str):
        """Save optimization results to JSON file"""
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
                'optimization_type': result.optimization_type,
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
        
        print(f"📁 Optimization results saved to {filename}")

def main():
    parser = argparse.ArgumentParser(description='Optimization Benchmark for CrabCache Phase 6.1')
    parser.add_argument('--host', default='127.0.0.1', help='CrabCache host')
    parser.add_argument('--port', type=int, default=8000, help='CrabCache port')
    parser.add_argument('--operations', type=int, default=200000, help='Total operations')
    parser.add_argument('--connections', type=int, default=32, help='Concurrent connections')
    parser.add_argument('--duration', type=int, default=30, help='Test duration in seconds')
    parser.add_argument('--target-ops', type=int, default=300000, help='Target ops/sec')
    parser.add_argument('--target-latency', type=float, default=1.0, help='Target P99 latency (ms)')
    parser.add_argument('--output', default='optimization_results.json', help='Output file')
    parser.add_argument('--skip-simd', action='store_true', help='Skip SIMD tests')
    parser.add_argument('--skip-zero-copy', action='store_true', help='Skip zero-copy tests')
    parser.add_argument('--skip-combined', action='store_true', help='Skip combined tests')
    
    args = parser.parse_args()
    
    config = OptimizationBenchmarkConfig(
        host=args.host,
        port=args.port,
        total_operations=args.operations,
        concurrent_connections=args.connections,
        test_duration_seconds=args.duration,
        target_ops_per_second=args.target_ops,
        target_p99_latency_ms=args.target_latency,
        test_simd_performance=not args.skip_simd,
        test_zero_copy_performance=not args.skip_zero_copy,
        test_combined_optimizations=not args.skip_combined,
    )
    
    benchmark = OptimizationBenchmark(config)
    
    try:
        # Test connection first
        test_conn = benchmark.create_connection()
        test_conn.close()
        print(f"✅ Connected to CrabCache at {args.host}:{args.port}")
        
        # Run optimization benchmarks
        results = benchmark.run_all_optimization_tests()
        
        # Print summary
        benchmark.print_optimization_summary()
        
        # Save results
        benchmark.save_optimization_results(args.output)
        
        # Check if target was met
        best_result = max(results, key=lambda r: r.ops_per_second)
        if best_result.ops_per_second >= args.target_ops:
            print(f"\n🎉 SUCCESS: Target of {args.target_ops:,} ops/sec achieved!")
            return 0
        else:
            print(f"\n⚠️  Target not met: {best_result.ops_per_second:,.0f} < {args.target_ops:,} ops/sec")
            return 1
        
    except Exception as e:
        print(f"❌ Optimization benchmark failed: {e}")
        return 1

if __name__ == "__main__":
    exit(main())