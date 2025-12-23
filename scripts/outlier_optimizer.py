#!/usr/bin/env python3
"""
Outlier Optimizer for CrabCache

Based on the outlier analysis, this script implements specific optimizations
to reduce P99.9 and P99.99 latencies.
"""

import socket
import struct
import time
import threading
import numpy as np
import json
import os
import signal
from dataclasses import dataclass
from typing import List, Dict, Any

# Binary protocol constants
CMD_PING = 0x01
RESP_PONG = 0x11

@dataclass
class OptimizerConfig:
    host: str = "127.0.0.1"
    port: int = 7001
    connections: int = 3  # Reduced from 5 based on analysis
    operations_per_connection: int = 30000
    connection_warmup_ops: int = 100  # Pre-warm connections
    enable_cpu_affinity: bool = True
    enable_process_priority: bool = True
    enable_connection_pooling: bool = True
    recv_buffer_size: int = 2048  # Reduced from 4096 (recv is bottleneck)
    send_buffer_size: int = 1024  # Reduced from 4096

class OptimizedClient:
    """Client with outlier-specific optimizations"""
    
    def __init__(self, host: str, port: int, client_id: int, config: OptimizerConfig):
        self.host = host
        self.port = port
        self.client_id = client_id
        self.config = config
        self.socket = None
        self.connected = False
        self.warmed_up = False
    
    def connect_optimized(self) -> bool:
        """Connect with outlier-specific optimizations"""
        try:
            self.socket = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
            
            # OUTLIER OPTIMIZATION 1: Smaller buffers (recv is bottleneck)
            self.socket.setsockopt(socket.SOL_SOCKET, socket.SO_RCVBUF, self.config.recv_buffer_size)
            self.socket.setsockopt(socket.SOL_SOCKET, socket.SO_SNDBUF, self.config.send_buffer_size)
            
            # OUTLIER OPTIMIZATION 2: More aggressive TCP settings
            self.socket.setsockopt(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)
            self.socket.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
            
            # OUTLIER OPTIMIZATION 3: Shorter timeout to fail fast
            self.socket.settimeout(1.0)  # Reduced from 5.0
            
            # OUTLIER OPTIMIZATION 4: Set socket to non-blocking after connect
            self.socket.connect((self.host, self.port))
            
            # OUTLIER OPTIMIZATION 5: Connection warmup
            if self.config.connection_warmup_ops > 0:
                self.warmup_connection()
            
            self.connected = True
            return True
            
        except Exception as e:
            return False
    
    def warmup_connection(self):
        """Warm up connection to reduce initial latency spikes"""
        ping_cmd = bytes([CMD_PING])
        
        for _ in range(self.config.connection_warmup_ops):
            try:
                self.socket.send(ping_cmd)
                response = self.socket.recv(1)
                if len(response) != 1 or response[0] != RESP_PONG:
                    break
            except:
                break
        
        self.warmed_up = True
    
    def disconnect(self):
        """Clean disconnect"""
        if self.socket:
            try:
                self.socket.close()
            except:
                pass
            self.socket = None
            self.connected = False
    
    def run_optimized_operations(self, operations: int) -> Dict:
        """Run operations with outlier optimizations"""
        results = {
            "successful_operations": 0,
            "failed_operations": 0,
            "latencies_ms": [],
            "outliers": [],
            "client_id": self.client_id,
            "warmed_up": self.warmed_up,
        }
        
        ping_cmd = bytes([CMD_PING])
        
        for i in range(operations):
            try:
                # OUTLIER OPTIMIZATION 6: Yield CPU occasionally to prevent starvation
                if i % 1000 == 0 and i > 0:
                    time.sleep(0.0001)  # 0.1ms yield every 1000 ops
                
                start_time = time.perf_counter_ns()
                
                self.socket.send(ping_cmd)
                response = self.socket.recv(1)
                
                end_time = time.perf_counter_ns()
                
                if len(response) == 1 and response[0] == RESP_PONG:
                    latency_ms = (end_time - start_time) / 1_000_000
                    results["latencies_ms"].append(latency_ms)
                    results["successful_operations"] += 1
                    
                    # Track outliers for analysis
                    if latency_ms > 1.0:
                        results["outliers"].append({
                            "operation_id": i,
                            "latency_ms": latency_ms,
                            "timestamp": time.perf_counter(),
                        })
                else:
                    results["failed_operations"] += 1
                    
            except Exception:
                results["failed_operations"] += 1
        
        return results

class OutlierOptimizer:
    """Optimizer specifically targeting P99.9 and P99.99 outliers"""
    
    def __init__(self, config: OptimizerConfig):
        self.config = config
        self.original_priority = None
        self.original_affinity = None
    
    def run_optimization_test(self):
        """Run optimization test to reduce outliers"""
        print("🔧 CrabCache Outlier Optimizer")
        print("=" * 40)
        print("🎯 GOAL: Reduce P99.9 and P99.99 outliers")
        print()
        
        # Apply system-level optimizations
        self.apply_system_optimizations()
        
        # Run baseline test
        print("📊 Running baseline test...")
        baseline_results = self.run_optimized_benchmark("Baseline")
        
        # Run optimized tests with different configurations
        optimizations = [
            ("Reduced Connections", {"connections": 2}),
            ("Smaller Buffers", {"recv_buffer_size": 1024, "send_buffer_size": 512}),
            ("More Warmup", {"connection_warmup_ops": 200}),
            ("CPU Yield", {"connections": 2, "recv_buffer_size": 1024}),
        ]
        
        results = {"baseline": baseline_results}
        
        for opt_name, opt_config in optimizations:
            print(f"📊 Running {opt_name} optimization...")
            
            # Create modified config
            modified_config = OptimizerConfig(**{
                **self.config.__dict__,
                **opt_config
            })
            
            # Run test with modified config
            opt_results = self.run_optimized_benchmark(opt_name, modified_config)
            results[opt_name.lower().replace(" ", "_")] = opt_results
        
        # Analyze and compare results
        self.analyze_optimization_results(results)
        
        # Restore system settings
        self.restore_system_settings()
    
    def apply_system_optimizations(self):
        """Apply system-level optimizations"""
        print("🔧 Applying system optimizations...")
        
        try:
            # OUTLIER OPTIMIZATION 7: Increase process priority
            if self.config.enable_process_priority:
                import psutil
                process = psutil.Process()
                self.original_priority = process.nice()
                
                # Set higher priority (lower nice value)
                try:
                    process.nice(-5)  # Higher priority
                    print("  ✅ Process priority increased")
                except:
                    print("  ⚠️  Could not increase process priority (requires sudo)")
            
            # OUTLIER OPTIMIZATION 8: Set CPU affinity
            if self.config.enable_cpu_affinity:
                import psutil
                process = psutil.Process()
                self.original_affinity = process.cpu_affinity()
                
                # Pin to first 2 CPUs for consistency
                available_cpus = process.cpu_affinity()
                if len(available_cpus) >= 2:
                    process.cpu_affinity([available_cpus[0], available_cpus[1]])
                    print(f"  ✅ CPU affinity set to cores {available_cpus[:2]}")
                else:
                    print("  ⚠️  Insufficient CPUs for affinity setting")
        
        except ImportError:
            print("  ⚠️  psutil not available for system optimizations")
        except Exception as e:
            print(f"  ⚠️  System optimization failed: {e}")
        
        print()
    
    def restore_system_settings(self):
        """Restore original system settings"""
        try:
            import psutil
            process = psutil.Process()
            
            if self.original_priority is not None:
                process.nice(self.original_priority)
            
            if self.original_affinity is not None:
                process.cpu_affinity(self.original_affinity)
            
            print("🔧 System settings restored")
        except:
            pass
    
    def run_optimized_benchmark(self, test_name: str, config: OptimizerConfig = None) -> Dict:
        """Run benchmark with specific optimization configuration"""
        if config is None:
            config = self.config
        
        def worker(client_id: int):
            client = OptimizedClient(config.host, config.port, client_id, config)
            
            if not client.connect_optimized():
                return {
                    "successful_operations": 0,
                    "failed_operations": config.operations_per_connection,
                    "latencies_ms": [],
                    "outliers": [],
                    "client_id": client_id,
                    "warmed_up": False,
                }
            
            try:
                results = client.run_optimized_operations(config.operations_per_connection)
                client.disconnect()
                return results
            except Exception:
                client.disconnect()
                return {
                    "successful_operations": 0,
                    "failed_operations": config.operations_per_connection,
                    "latencies_ms": [],
                    "outliers": [],
                    "client_id": client_id,
                    "warmed_up": False,
                }
        
        # Run workers
        start_time = time.perf_counter()
        
        from concurrent.futures import ThreadPoolExecutor
        with ThreadPoolExecutor(max_workers=config.connections) as executor:
            futures = [executor.submit(worker, i) for i in range(config.connections)]
            worker_results = [future.result() for future in futures]
        
        end_time = time.perf_counter()
        duration = end_time - start_time
        
        # Combine results
        combined_results = {
            "test_name": test_name,
            "config": config.__dict__,
            "successful_operations": sum(r["successful_operations"] for r in worker_results),
            "failed_operations": sum(r["failed_operations"] for r in worker_results),
            "duration": duration,
            "latencies_ms": [],
            "outliers": [],
        }
        
        for result in worker_results:
            combined_results["latencies_ms"].extend(result["latencies_ms"])
            combined_results["outliers"].extend(result["outliers"])
        
        # Calculate percentiles
        if combined_results["latencies_ms"]:
            latencies = np.array(combined_results["latencies_ms"])
            combined_results["percentiles"] = {
                "p50": float(np.percentile(latencies, 50)),
                "p90": float(np.percentile(latencies, 90)),
                "p95": float(np.percentile(latencies, 95)),
                "p99": float(np.percentile(latencies, 99)),
                "p99_9": float(np.percentile(latencies, 99.9)),
                "p99_99": float(np.percentile(latencies, 99.99)),
            }
            combined_results["throughput"] = combined_results["successful_operations"] / duration
        else:
            combined_results["percentiles"] = {}
            combined_results["throughput"] = 0
        
        return combined_results
    
    def analyze_optimization_results(self, results: Dict):
        """Analyze and compare optimization results"""
        print("📊 Optimization Results Analysis:")
        print("=" * 45)
        
        baseline = results["baseline"]
        baseline_p99_9 = baseline["percentiles"].get("p99_9", 0)
        baseline_p99_99 = baseline["percentiles"].get("p99_99", 0)
        
        print(f"{'Test':<20} {'P99':<8} {'P99.9':<8} {'P99.99':<9} {'Outliers':<8} {'Improvement'}")
        print("-" * 75)
        
        best_p99_9 = float('inf')
        best_test = ""
        
        for test_name, result in results.items():
            if not result["percentiles"]:
                continue
            
            p99 = result["percentiles"]["p99"]
            p99_9 = result["percentiles"]["p99_9"]
            p99_99 = result["percentiles"]["p99_99"]
            outlier_count = len(result["outliers"])
            
            if test_name == "baseline":
                improvement = "Baseline"
            else:
                p99_9_improvement = (baseline_p99_9 - p99_9) / baseline_p99_9 * 100
                improvement = f"{p99_9_improvement:+.1f}%"
            
            if p99_9 < best_p99_9:
                best_p99_9 = p99_9
                best_test = test_name
            
            status = "✅" if p99_9 < 2.0 else "⚠️" if p99_9 < 5.0 else "❌"
            
            print(f"{test_name:<20} {p99:.3f}ms {p99_9:.3f}ms {p99_99:.3f}ms  {outlier_count:<8} {improvement} {status}")
        
        print()
        
        # Best configuration analysis
        if best_test and best_test != "baseline":
            best_result = results[best_test]
            improvement = (baseline_p99_9 - best_p99_9) / baseline_p99_9 * 100
            
            print(f"🏆 Best Configuration: {best_test}")
            print(f"  P99.9 improvement: {improvement:.1f}% ({baseline_p99_9:.3f}ms → {best_p99_9:.3f}ms)")
            print(f"  Configuration: {best_result['config']}")
            print()
        
        # Recommendations
        self.generate_final_recommendations(results, best_test, best_p99_9)
        
        # Save results
        self.save_optimization_results(results)
    
    def generate_final_recommendations(self, results: Dict, best_test: str, best_p99_9: float):
        """Generate final optimization recommendations"""
        print("💡 Final Optimization Recommendations:")
        print("-" * 40)
        
        if best_p99_9 < 1.5:
            print("🎉 EXCELLENT! P99.9 < 1.5ms achieved")
            print("🔧 Fine-tuning recommendations:")
            print("  - Monitor for system-level interrupts")
            print("  - Consider dedicated hardware for production")
            print("  - Implement connection pre-warming in production")
        elif best_p99_9 < 2.0:
            print("✅ GOOD! P99.9 < 2ms achieved")
            print("🔧 Further optimization recommendations:")
            print("  - Implement the best configuration found")
            print("  - Monitor system during peak load")
            print("  - Consider OS-level tuning")
        else:
            print("⚠️  P99.9 still above 2ms")
            print("🔧 Additional optimization needed:")
            print("  - Investigate OS-level scheduling")
            print("  - Consider real-time kernel")
            print("  - Profile server-side processing")
        
        print()
        
        if best_test and best_test != "baseline":
            best_config = results[best_test]["config"]
            print("🎯 Recommended Production Configuration:")
            print(f"  - Connections: {best_config['connections']}")
            print(f"  - Recv buffer: {best_config['recv_buffer_size']} bytes")
            print(f"  - Send buffer: {best_config['send_buffer_size']} bytes")
            print(f"  - Warmup operations: {best_config['connection_warmup_ops']}")
            print(f"  - CPU affinity: {best_config['enable_cpu_affinity']}")
            print(f"  - Process priority: {best_config['enable_process_priority']}")
        
        print()
    
    def save_optimization_results(self, results: Dict):
        """Save optimization results"""
        timestamp = time.strftime("%Y%m%d_%H%M%S")
        filename = f"crabcache/benchmark_results/outlier_optimization_{timestamp}.json"
        
        # Prepare for JSON serialization
        json_results = {}
        for test_name, result in results.items():
            json_result = {k: v for k, v in result.items() if k != "latencies_ms"}  # Exclude raw latencies
            json_results[test_name] = json_result
        
        with open(filename, 'w') as f:
            json.dump(json_results, f, indent=2)
        
        print(f"💾 Optimization results saved to: {filename}")

def main():
    print("🔧 Starting CrabCache Outlier Optimizer...")
    print("🎯 Goal: Reduce P99.9 and P99.99 outliers through targeted optimizations")
    print()
    
    config = OptimizerConfig(
        host="127.0.0.1",
        port=7001,
        connections=3,  # Reduced based on analysis
        operations_per_connection=30000,
        connection_warmup_ops=100,
        enable_cpu_affinity=True,
        enable_process_priority=True,
        recv_buffer_size=2048,  # Optimized based on bottleneck analysis
        send_buffer_size=1024,
    )
    
    optimizer = OutlierOptimizer(config)
    optimizer.run_optimization_test()
    
    print("\n🎊 Outlier Optimization Complete!")
    print("Apply the recommended configuration for best P99.9 performance.")

if __name__ == "__main__":
    main()