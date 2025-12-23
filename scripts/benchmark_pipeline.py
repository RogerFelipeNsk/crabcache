#!/usr/bin/env python3
"""
CrabCache Pipeline Performance Benchmark

This script tests the performance improvements achieved by pipelining
compared to single command processing.
"""

import asyncio
import socket
import time
import statistics
import json
import sys
from datetime import datetime
from typing import List, Dict, Any, Tuple

class CrabCacheClient:
    """Simple CrabCache client for benchmarking"""
    
    def __init__(self, host: str = "127.0.0.1", port: int = 8000):
        self.host = host
        self.port = port
        self.socket = None
    
    async def connect(self):
        """Connect to CrabCache server"""
        self.socket = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        self.socket.setsockopt(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)
        self.socket.connect((self.host, self.port))
        print(f"Connected to CrabCache at {self.host}:{self.port}")
    
    def disconnect(self):
        """Disconnect from server"""
        if self.socket:
            self.socket.close()
            self.socket = None
    
    def send_command(self, command: str) -> str:
        """Send single command and receive response"""
        if not self.socket:
            raise RuntimeError("Not connected")
        
        # Send command
        self.socket.send(f"{command}\n".encode())
        
        # Receive response
        response = b""
        while b"\n" not in response:
            chunk = self.socket.recv(1024)
            if not chunk:
                break
            response += chunk
        
        return response.decode().strip()
    
    def send_pipeline_batch(self, commands: List[str]) -> List[str]:
        """Send batch of commands using pipelining"""
        if not self.socket:
            raise RuntimeError("Not connected")
        
        # Send all commands at once
        batch_data = "".join(f"{cmd}\n" for cmd in commands)
        self.socket.send(batch_data.encode())
        
        # Receive all responses properly
        responses = []
        response_buffer = b""
        
        while len(responses) < len(commands):
            chunk = self.socket.recv(4096)
            if not chunk:
                break
            response_buffer += chunk
            
            # Split by newlines and extract complete responses
            while b"\n" in response_buffer:
                line, response_buffer = response_buffer.split(b"\n", 1)
                if line:  # Skip empty lines
                    responses.append(line.decode().strip())
                if len(responses) >= len(commands):
                    break
        
        return responses

class PipelineBenchmark:
    """Pipeline performance benchmark suite"""
    
    def __init__(self, host: str = "127.0.0.1", port: int = 8000):
        self.host = host
        self.port = port
        self.client = CrabCacheClient(host, port)
    
    async def setup(self):
        """Setup benchmark environment"""
        await self.client.connect()
        
        # Clear any existing data
        try:
            self.client.send_command("PING")
            print("✓ Server is responding")
        except Exception as e:
            print(f"✗ Failed to connect to server: {e}")
            sys.exit(1)
    
    def cleanup(self):
        """Cleanup benchmark environment"""
        self.client.disconnect()
    
    def benchmark_single_commands(self, num_operations: int) -> Dict[str, Any]:
        """Benchmark single command processing (no pipelining)"""
        print(f"\n🔄 Benchmarking single commands ({num_operations} operations)...")
        
        latencies = []
        start_time = time.time()
        
        for i in range(num_operations):
            key = f"key_{i}"
            value = f"value_{i}"
            
            # Measure individual command latency
            cmd_start = time.time()
            response = self.client.send_command(f"PUT {key} {value}")
            cmd_end = time.time()
            
            latencies.append((cmd_end - cmd_start) * 1000)  # Convert to ms
            
            if response != "OK":
                print(f"Warning: Unexpected response: {response}")
        
        end_time = time.time()
        total_time = end_time - start_time
        ops_per_sec = num_operations / total_time
        
        return {
            "mode": "single_commands",
            "total_operations": num_operations,
            "total_time_seconds": total_time,
            "ops_per_second": ops_per_sec,
            "avg_latency_ms": statistics.mean(latencies),
            "p50_latency_ms": statistics.median(latencies),
            "p95_latency_ms": statistics.quantiles(latencies, n=20)[18],  # 95th percentile
            "p99_latency_ms": statistics.quantiles(latencies, n=100)[98],  # 99th percentile
            "min_latency_ms": min(latencies),
            "max_latency_ms": max(latencies),
        }
    
    def benchmark_pipeline_batch(self, num_operations: int, batch_size: int) -> Dict[str, Any]:
        """Benchmark pipeline batch processing"""
        print(f"\n🚀 Benchmarking pipeline batches ({num_operations} operations, batch_size={batch_size})...")
        
        batch_latencies = []
        start_time = time.time()
        
        num_batches = (num_operations + batch_size - 1) // batch_size  # Ceiling division
        
        for batch_idx in range(num_batches):
            # Create batch of commands
            commands = []
            batch_start_idx = batch_idx * batch_size
            batch_end_idx = min(batch_start_idx + batch_size, num_operations)
            
            for i in range(batch_start_idx, batch_end_idx):
                key = f"key_{i}"
                value = f"value_{i}"
                commands.append(f"PUT {key} {value}")
            
            # Measure batch latency
            batch_start = time.time()
            responses = self.client.send_pipeline_batch(commands)
            batch_end = time.time()
            
            batch_latencies.append((batch_end - batch_start) * 1000)  # Convert to ms
            
            # Verify responses
            for response in responses:
                if response != "OK":
                    print(f"Warning: Unexpected response: {response}")
        
        end_time = time.time()
        total_time = end_time - start_time
        ops_per_sec = num_operations / total_time
        
        # Calculate per-command latency (batch latency / batch size)
        per_command_latencies = []
        for batch_latency in batch_latencies:
            per_command_latencies.extend([batch_latency / batch_size] * batch_size)
        
        return {
            "mode": "pipeline_batch",
            "batch_size": batch_size,
            "total_operations": num_operations,
            "total_batches": num_batches,
            "total_time_seconds": total_time,
            "ops_per_second": ops_per_sec,
            "avg_batch_latency_ms": statistics.mean(batch_latencies),
            "avg_per_command_latency_ms": statistics.mean(per_command_latencies),
            "p50_latency_ms": statistics.median(per_command_latencies),
            "p95_latency_ms": statistics.quantiles(per_command_latencies, n=20)[18],
            "p99_latency_ms": statistics.quantiles(per_command_latencies, n=100)[98],
            "min_latency_ms": min(per_command_latencies),
            "max_latency_ms": max(per_command_latencies),
        }
    
    def benchmark_mixed_workload(self, num_operations: int, batch_size: int) -> Dict[str, Any]:
        """Benchmark mixed workload (PUT, GET, DEL) with pipelining"""
        print(f"\n🔀 Benchmarking mixed workload ({num_operations} operations, batch_size={batch_size})...")
        
        batch_latencies = []
        start_time = time.time()
        
        num_batches = (num_operations + batch_size - 1) // batch_size
        
        for batch_idx in range(num_batches):
            commands = []
            batch_start_idx = batch_idx * batch_size
            batch_end_idx = min(batch_start_idx + batch_size, num_operations)
            
            for i in range(batch_start_idx, batch_end_idx):
                key = f"key_{i}"
                value = f"value_{i}"
                
                # Mix of operations: 50% PUT, 30% GET, 20% DEL
                op_type = i % 10
                if op_type < 5:  # 50% PUT
                    commands.append(f"PUT {key} {value}")
                elif op_type < 8:  # 30% GET
                    commands.append(f"GET {key}")
                else:  # 20% DEL
                    commands.append(f"DEL {key}")
            
            # Measure batch latency
            batch_start = time.time()
            responses = self.client.send_pipeline_batch(commands)
            batch_end = time.time()
            
            batch_latencies.append((batch_end - batch_start) * 1000)
        
        end_time = time.time()
        total_time = end_time - start_time
        ops_per_sec = num_operations / total_time
        
        per_command_latencies = []
        for batch_latency in batch_latencies:
            per_command_latencies.extend([batch_latency / batch_size] * batch_size)
        
        return {
            "mode": "mixed_workload_pipeline",
            "batch_size": batch_size,
            "total_operations": num_operations,
            "total_batches": num_batches,
            "total_time_seconds": total_time,
            "ops_per_second": ops_per_sec,
            "avg_batch_latency_ms": statistics.mean(batch_latencies),
            "avg_per_command_latency_ms": statistics.mean(per_command_latencies),
            "p50_latency_ms": statistics.median(per_command_latencies),
            "p95_latency_ms": statistics.quantiles(per_command_latencies, n=20)[18],
            "p99_latency_ms": statistics.quantiles(per_command_latencies, n=100)[98],
        }
    
    def run_comprehensive_benchmark(self) -> Dict[str, Any]:
        """Run comprehensive pipeline benchmark suite"""
        print("🚀 Starting CrabCache Pipeline Performance Benchmark")
        print("=" * 60)
        
        results = {
            "timestamp": datetime.now().isoformat(),
            "server": f"{self.host}:{self.port}",
            "benchmarks": []
        }
        
        # Test configurations
        num_operations = 1000  # Reduced from 10000
        batch_sizes = [1, 4, 8, 16]  # Reduced batch sizes
        
        try:
            # 1. Single command baseline
            single_result = self.benchmark_single_commands(num_operations)
            results["benchmarks"].append(single_result)
            
            # 2. Pipeline batches with different sizes
            for batch_size in batch_sizes[1:]:  # Skip batch_size=1 (same as single)
                pipeline_result = self.benchmark_pipeline_batch(num_operations, batch_size)
                results["benchmarks"].append(pipeline_result)
            
            # 3. Mixed workload with optimal batch size
            optimal_batch_size = 16
            mixed_result = self.benchmark_mixed_workload(num_operations, optimal_batch_size)
            results["benchmarks"].append(mixed_result)
            
            # Calculate performance improvements
            baseline_ops = single_result["ops_per_second"]
            best_pipeline = max(results["benchmarks"][1:], key=lambda x: x["ops_per_second"])
            improvement_factor = best_pipeline["ops_per_second"] / baseline_ops
            
            results["summary"] = {
                "baseline_ops_per_second": baseline_ops,
                "best_pipeline_ops_per_second": best_pipeline["ops_per_second"],
                "best_batch_size": best_pipeline.get("batch_size", "N/A"),
                "improvement_factor": improvement_factor,
                "improvement_percentage": (improvement_factor - 1) * 100,
            }
            
            return results
            
        except Exception as e:
            print(f"❌ Benchmark failed: {e}")
            raise
    
    def print_results(self, results: Dict[str, Any]):
        """Print benchmark results in a readable format"""
        print("\n" + "=" * 80)
        print("📊 CRABCACHE PIPELINE BENCHMARK RESULTS")
        print("=" * 80)
        
        print(f"🕐 Timestamp: {results['timestamp']}")
        print(f"🌐 Server: {results['server']}")
        print()
        
        # Print individual benchmark results
        for benchmark in results["benchmarks"]:
            mode = benchmark["mode"]
            ops_per_sec = benchmark["ops_per_second"]
            avg_latency = benchmark["avg_per_command_latency_ms"] if "avg_per_command_latency_ms" in benchmark else benchmark["avg_latency_ms"]
            p99_latency = benchmark["p99_latency_ms"]
            
            print(f"📈 {mode.upper().replace('_', ' ')}")
            if "batch_size" in benchmark:
                print(f"   Batch Size: {benchmark['batch_size']}")
            print(f"   Operations/sec: {ops_per_sec:,.0f}")
            print(f"   Avg Latency: {avg_latency:.2f}ms")
            print(f"   P99 Latency: {p99_latency:.2f}ms")
            print()
        
        # Print summary
        if "summary" in results:
            summary = results["summary"]
            print("🏆 PERFORMANCE SUMMARY")
            print("-" * 40)
            print(f"Baseline (single commands): {summary['baseline_ops_per_second']:,.0f} ops/sec")
            print(f"Best pipeline performance: {summary['best_pipeline_ops_per_second']:,.0f} ops/sec")
            print(f"Best batch size: {summary['best_batch_size']}")
            print(f"Performance improvement: {summary['improvement_factor']:.1f}x ({summary['improvement_percentage']:.1f}%)")
            print()
            
            # Compare with Redis
            redis_baseline = 37498  # Redis with pipelining from documentation
            if summary['best_pipeline_ops_per_second'] > redis_baseline:
                redis_factor = summary['best_pipeline_ops_per_second'] / redis_baseline
                print(f"🚀 CrabCache vs Redis: {redis_factor:.1f}x FASTER! 🏆")
            else:
                redis_factor = redis_baseline / summary['best_pipeline_ops_per_second']
                print(f"📊 CrabCache vs Redis: {redis_factor:.1f}x slower")
            print()

async def main():
    """Main benchmark execution"""
    import argparse
    
    parser = argparse.ArgumentParser(description="CrabCache Pipeline Performance Benchmark")
    parser.add_argument("--host", default="127.0.0.1", help="CrabCache server host")
    parser.add_argument("--port", type=int, default=8000, help="CrabCache server port")
    parser.add_argument("--output", help="Output file for JSON results")
    
    args = parser.parse_args()
    
    benchmark = PipelineBenchmark(args.host, args.port)
    
    try:
        await benchmark.setup()
        results = benchmark.run_comprehensive_benchmark()
        benchmark.print_results(results)
        
        # Save results to file if specified
        if args.output:
            with open(args.output, 'w') as f:
                json.dump(results, f, indent=2)
            print(f"💾 Results saved to {args.output}")
        
        # Save results with timestamp
        timestamp = datetime.now().strftime("%Y%m%d_%H%M%S")
        filename = f"benchmark_results/pipeline_benchmark_{timestamp}.json"
        with open(filename, 'w') as f:
            json.dump(results, f, indent=2)
        print(f"💾 Results saved to {filename}")
        
    except KeyboardInterrupt:
        print("\n⏹️  Benchmark interrupted by user")
    except Exception as e:
        print(f"❌ Benchmark failed: {e}")
        sys.exit(1)
    finally:
        benchmark.cleanup()

if __name__ == "__main__":
    asyncio.run(main())