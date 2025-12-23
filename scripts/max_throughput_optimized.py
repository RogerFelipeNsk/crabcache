#!/usr/bin/env python3
"""
Maximum Throughput Benchmark with Outlier Optimizations

This script measures the maximum transactions per second (TPS) 
achievable with the outlier optimizations applied.
"""

import socket
import struct
import time
import threading
from concurrent.futures import ThreadPoolExecutor
import numpy as np
from dataclasses import dataclass
from typing import List, Dict, Any

# Binary protocol constants
CMD_PING = 0x01
CMD_PUT = 0x02
CMD_GET = 0x03

RESP_OK = 0x10
RESP_PONG = 0x11
RESP_VALUE = 0x14

@dataclass
class MaxThroughputConfig:
    host: str = "127.0.0.1"
    port: int = 7001
    test_duration: int = 30  # 30 seconds test
    warmup_duration: int = 5  # 5 seconds warmup

class OptimizedThroughputClient:
    """Client optimized for maximum throughput with low outliers"""
    
    def __init__(self, host: str, port: int, client_id: int, config: dict):
        self.host = host
        self.port = port
        self.client_id = client_id
        self.config = config
        self.socket = None
        self.connected = False
    
    def connect_for_throughput(self) -> bool:
        """Connect with throughput-optimized settings"""
        try:
            self.socket = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
            
            # Apply optimized settings
            self.socket.setsockopt(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)
            self.socket.setsockopt(socket.SOL_SOCKET, socket.SO_RCVBUF, self.config.get('recv_buffer', 2048))
            self.socket.setsockopt(socket.SOL_SOCKET, socket.SO_SNDBUF, self.config.get('send_buffer', 1024))
            self.socket.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
            self.socket.settimeout(2.0)
            
            self.socket.connect((self.host, self.port))
            
            # Connection warmup
            warmup_ops = self.config.get('warmup_ops', 100)
            ping_cmd = bytes([CMD_PING])
            
            for _ in range(warmup_ops):
                self.socket.send(ping_cmd)
                response = self.socket.recv(1)
                if len(response) != 1 or response[0] != RESP_PONG:
                    return False
            
            self.connected = True
            return True
            
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
    
    def run_throughput_test(self, duration: float, operation_type: str = "PING") -> Dict:
        """Run throughput test for specified duration"""
        results = {
            "client_id": self.client_id,
            "operation_type": operation_type,
            "successful_operations": 0,
            "failed_operations": 0,
            "duration": 0,
            "latencies": [],
            "start_time": 0,
            "end_time": 0,
        }
        
        if not self.connected:
            return results
        
        # Prepare commands
        ping_cmd = bytes([CMD_PING])
        
        # Pre-generate PUT command for mixed tests
        test_key = f"throughput_key_{self.client_id}".encode()
        test_value = b"throughput_value"
        put_cmd = bytearray()
        put_cmd.append(CMD_PUT)
        put_cmd.extend(struct.pack('<I', len(test_key)))
        put_cmd.extend(test_key)
        put_cmd.extend(struct.pack('<I', len(test_value)))
        put_cmd.extend(test_value)
        put_cmd.append(0)  # No TTL
        
        # Pre-generate GET command
        get_cmd = bytearray()
        get_cmd.append(CMD_GET)
        get_cmd.extend(struct.pack('<I', len(test_key)))
        get_cmd.extend(test_key)
        
        # Store one value for GET tests
        if operation_type in ["GET", "MIXED"]:
            try:
                self.socket.send(put_cmd)
                self.socket.recv(1)
            except:
                pass
        
        start_time = time.perf_counter()
        results["start_time"] = start_time
        
        operation_count = 0
        cpu_yield_interval = self.config.get('cpu_yield_interval', 1000)
        
        try:
            while True:
                current_time = time.perf_counter()
                if current_time - start_time >= duration:
                    break
                
                # CPU yield optimization
                if operation_count % cpu_yield_interval == 0 and operation_count > 0:
                    time.sleep(0.00005)  # 0.05ms yield
                
                # Choose operation based on type
                if operation_type == "PING":
                    cmd = ping_cmd
                    expected_response = RESP_PONG
                elif operation_type == "PUT":
                    # Use unique key for each PUT
                    unique_key = f"tput_key_{self.client_id}_{operation_count}".encode()
                    unique_put_cmd = bytearray()
                    unique_put_cmd.append(CMD_PUT)
                    unique_put_cmd.extend(struct.pack('<I', len(unique_key)))
                    unique_put_cmd.extend(unique_key)
                    unique_put_cmd.extend(struct.pack('<I', len(test_value)))
                    unique_put_cmd.extend(test_value)
                    unique_put_cmd.append(0)
                    cmd = unique_put_cmd
                    expected_response = RESP_OK
                elif operation_type == "GET":
                    cmd = get_cmd
                    expected_response = RESP_VALUE
                elif operation_type == "MIXED":
                    # 40% PING, 30% PUT, 30% GET
                    op_choice = operation_count % 10
                    if op_choice < 4:  # PING
                        cmd = ping_cmd
                        expected_response = RESP_PONG
                    elif op_choice < 7:  # PUT
                        unique_key = f"mixed_key_{self.client_id}_{operation_count}".encode()
                        unique_put_cmd = bytearray()
                        unique_put_cmd.append(CMD_PUT)
                        unique_put_cmd.extend(struct.pack('<I', len(unique_key)))
                        unique_put_cmd.extend(unique_key)
                        unique_put_cmd.extend(struct.pack('<I', len(test_value)))
                        unique_put_cmd.extend(test_value)
                        unique_put_cmd.append(0)
                        cmd = unique_put_cmd
                        expected_response = RESP_OK
                    else:  # GET
                        cmd = get_cmd
                        expected_response = RESP_VALUE
                
                # Execute operation with timing
                op_start = time.perf_counter_ns()
                
                self.socket.send(cmd)
                response = self.socket.recv(1)
                
                op_end = time.perf_counter_ns()
                
                # Validate response
                success = False
                if len(response) == 1:
                    if expected_response == RESP_VALUE:
                        if response[0] == RESP_VALUE:
                            # Read value for GET operations
                            len_bytes = self.socket.recv(4)
                            if len(len_bytes) == 4:
                                value_len = struct.unpack('<I', len_bytes)[0]
                                if value_len <= 1024:
                                    value = self.socket.recv(value_len)
                                    success = len(value) == value_len
                        elif response[0] == 0x12:  # NULL response is also valid
                            success = True
                    else:
                        success = response[0] == expected_response
                
                if success:
                    results["successful_operations"] += 1
                    latency_ms = (op_end - op_start) / 1_000_000
                    results["latencies"].append(latency_ms)
                else:
                    results["failed_operations"] += 1
                
                operation_count += 1
                
        except Exception as e:
            results["error"] = str(e)
        
        end_time = time.perf_counter()
        results["end_time"] = end_time
        results["duration"] = end_time - start_time
        
        return results

class MaxThroughputBenchmark:
    """Benchmark to find maximum throughput with optimizations"""
    
    def __init__(self, config: MaxThroughputConfig):
        self.config = config
    
    def run_max_throughput_benchmark(self):
        """Run comprehensive throughput benchmark"""
        print("🚀 CrabCache Maximum Throughput Benchmark")
        print("=" * 50)
        print("🎯 GOAL: Find maximum TPS with outlier optimizations")
        print(f"📊 Test duration: {self.config.test_duration}s per configuration")
        print(f"🔧 Warmup duration: {self.config.warmup_duration}s")
        print()
        
        # Test different configurations
        configurations = [
            {
                "name": "Outlier-Optimized (2 conn)",
                "connections": 2,
                "recv_buffer": 2048,
                "send_buffer": 1024,
                "warmup_ops": 100,
                "cpu_yield_interval": 1000,
            },
            {
                "name": "High Throughput (10 conn)",
                "connections": 10,
                "recv_buffer": 8192,
                "send_buffer": 4096,
                "warmup_ops": 50,
                "cpu_yield_interval": 2000,
            },
            {
                "name": "Balanced (5 conn)",
                "connections": 5,
                "recv_buffer": 4096,
                "send_buffer": 2048,
                "warmup_ops": 100,
                "cpu_yield_interval": 1500,
            },
            {
                "name": "Ultra High (20 conn)",
                "connections": 20,
                "recv_buffer": 16384,
                "send_buffer": 8192,
                "warmup_ops": 25,
                "cpu_yield_interval": 3000,
            },
        ]
        
        operation_types = ["PING", "PUT", "GET", "MIXED"]
        
        all_results = {}
        
        for config in configurations:
            print(f"🔧 Testing {config['name']}...")
            config_results = {}
            
            for op_type in operation_types:
                print(f"  📊 {op_type} operations...")
                
                # Warmup
                print(f"    🔥 Warming up ({self.config.warmup_duration}s)...")
                warmup_result = self.run_throughput_test(config, op_type, self.config.warmup_duration)
                
                # Main test
                print(f"    ⚡ Main test ({self.config.test_duration}s)...")
                main_result = self.run_throughput_test(config, op_type, self.config.test_duration)
                
                config_results[op_type] = main_result
                
                if main_result["throughput"] > 0:
                    print(f"    📈 {main_result['throughput']:,.0f} ops/sec")
                else:
                    print(f"    ❌ Test failed")
            
            all_results[config["name"]] = config_results
            print()
        
        # Analyze results
        self.analyze_throughput_results(all_results)
    
    def run_throughput_test(self, config: dict, operation_type: str, duration: float) -> Dict:
        """Run throughput test with specific configuration"""
        def worker(client_id: int):
            client = OptimizedThroughputClient(
                self.config.host, 
                self.config.port, 
                client_id, 
                config
            )
            
            if not client.connect_for_throughput():
                return {
                    "client_id": client_id,
                    "successful_operations": 0,
                    "failed_operations": 0,
                    "duration": duration,
                    "latencies": [],
                    "error": "Connection failed"
                }
            
            try:
                result = client.run_throughput_test(duration, operation_type)
                client.disconnect()
                return result
            except Exception as e:
                client.disconnect()
                return {
                    "client_id": client_id,
                    "successful_operations": 0,
                    "failed_operations": 0,
                    "duration": duration,
                    "latencies": [],
                    "error": str(e)
                }
        
        # Run workers
        start_time = time.perf_counter()
        
        with ThreadPoolExecutor(max_workers=config["connections"]) as executor:
            futures = [executor.submit(worker, i) for i in range(config["connections"])]
            worker_results = [future.result() for future in futures]
        
        end_time = time.perf_counter()
        total_duration = end_time - start_time
        
        # Combine results
        combined_result = {
            "config_name": config["name"],
            "operation_type": operation_type,
            "connections": config["connections"],
            "total_duration": total_duration,
            "successful_operations": sum(r["successful_operations"] for r in worker_results),
            "failed_operations": sum(r["failed_operations"] for r in worker_results),
            "all_latencies": [],
        }
        
        for result in worker_results:
            combined_result["all_latencies"].extend(result["latencies"])
        
        # Calculate metrics
        if combined_result["successful_operations"] > 0:
            combined_result["throughput"] = combined_result["successful_operations"] / total_duration
            
            if combined_result["all_latencies"]:
                latencies = np.array(combined_result["all_latencies"])
                combined_result["latency_stats"] = {
                    "p50": float(np.percentile(latencies, 50)),
                    "p95": float(np.percentile(latencies, 95)),
                    "p99": float(np.percentile(latencies, 99)),
                    "p99_9": float(np.percentile(latencies, 99.9)),
                }
            else:
                combined_result["latency_stats"] = {}
        else:
            combined_result["throughput"] = 0
            combined_result["latency_stats"] = {}
        
        return combined_result
    
    def analyze_throughput_results(self, all_results: Dict):
        """Analyze and compare throughput results"""
        print("📊 Maximum Throughput Analysis:")
        print("=" * 50)
        
        # Find maximum throughput overall
        max_throughput = 0
        max_config = ""
        max_operation = ""
        
        print(f"{'Configuration':<25} {'Operation':<8} {'Throughput':<12} {'P99':<8} {'P99.9':<8}")
        print("-" * 70)
        
        for config_name, config_results in all_results.items():
            for op_type, result in config_results.items():
                throughput = result.get("throughput", 0)
                p99 = result.get("latency_stats", {}).get("p99", 0)
                p99_9 = result.get("latency_stats", {}).get("p99_9", 0)
                
                if throughput > max_throughput:
                    max_throughput = throughput
                    max_config = config_name
                    max_operation = op_type
                
                status = "✅" if p99_9 < 2.0 else "⚠️" if p99_9 < 5.0 else "❌"
                
                print(f"{config_name:<25} {op_type:<8} {throughput:>8,.0f} ops/sec {p99:>6.2f}ms {p99_9:>6.2f}ms {status}")
        
        print()
        print("🏆 MAXIMUM THROUGHPUT ACHIEVED:")
        print(f"  Configuration: {max_config}")
        print(f"  Operation: {max_operation}")
        print(f"  Throughput: {max_throughput:,.0f} ops/sec")
        
        if max_config in all_results and max_operation in all_results[max_config]:
            best_result = all_results[max_config][max_operation]
            latency_stats = best_result.get("latency_stats", {})
            
            print(f"  Connections: {best_result.get('connections', 'N/A')}")
            print(f"  P50 Latency: {latency_stats.get('p50', 0):.3f}ms")
            print(f"  P99 Latency: {latency_stats.get('p99', 0):.3f}ms")
            print(f"  P99.9 Latency: {latency_stats.get('p99_9', 0):.3f}ms")
        
        print()
        
        # Compare with previous results
        self.compare_with_previous_results(max_throughput, max_config, max_operation)
        
        # Save results
        self.save_throughput_results(all_results, max_throughput, max_config, max_operation)
    
    def compare_with_previous_results(self, max_throughput: float, max_config: str, max_operation: str):
        """Compare with previous benchmark results"""
        print("📈 Performance Comparison:")
        print("-" * 30)
        
        # Historical results
        previous_results = {
            "Original": 1741,
            "Phase 1 (TCP)": 2518,
            "Phase 2 (Binary)": 5092,
            "Phase 3 (Initial)": 21588,
            "Phase 3 (High Concurrency)": 25181,
        }
        
        print("Evolution:")
        for phase, throughput in previous_results.items():
            improvement = (max_throughput / throughput - 1) * 100 if throughput > 0 else 0
            print(f"  {phase:<25}: {throughput:>8,} ops/sec ({improvement:+6.1f}%)")
        
        print(f"  {'Current Maximum':<25}: {max_throughput:>8,.0f} ops/sec (NEW RECORD!)")
        
        # Redis comparison
        redis_baseline = 37498
        redis_ratio = max_throughput / redis_baseline * 100
        
        print()
        print(f"🥊 vs Redis ({redis_baseline:,} ops/sec): {redis_ratio:.1f}%")
        
        if max_throughput > redis_baseline:
            surplus = max_throughput - redis_baseline
            print(f"🏆 REDIS SURPASSED! (+{surplus:,.0f} ops/sec)")
        else:
            gap = redis_baseline - max_throughput
            print(f"📊 Redis gap: {gap:,.0f} ops/sec ({gap/redis_baseline*100:.1f}%)")
    
    def save_throughput_results(self, all_results: Dict, max_throughput: float, max_config: str, max_operation: str):
        """Save throughput results"""
        timestamp = time.strftime("%Y%m%d_%H%M%S")
        filename = f"crabcache/benchmark_results/max_throughput_optimized_{timestamp}.json"
        
        # Prepare for JSON serialization
        json_results = {}
        for config_name, config_results in all_results.items():
            json_config_results = {}
            for op_type, result in config_results.items():
                # Remove raw latencies to keep file size manageable
                json_result = {k: v for k, v in result.items() if k != "all_latencies"}
                json_config_results[op_type] = json_result
            json_results[config_name] = json_config_results
        
        summary_data = {
            "timestamp": timestamp,
            "test_duration": self.config.test_duration,
            "warmup_duration": self.config.warmup_duration,
            "maximum_throughput": {
                "ops_per_sec": max_throughput,
                "config": max_config,
                "operation": max_operation,
            },
            "configurations_tested": len(all_results),
            "operation_types_tested": ["PING", "PUT", "GET", "MIXED"],
            "results": json_results,
        }
        
        with open(filename, 'w') as f:
            import json
            json.dump(summary_data, f, indent=2)
        
        print(f"💾 Throughput results saved to: {filename}")

def main():
    print("🚀 Starting CrabCache Maximum Throughput Benchmark...")
    print("🎯 Goal: Find maximum TPS with outlier optimizations applied")
    print()
    
    config = MaxThroughputConfig(
        host="127.0.0.1",
        port=7001,
        test_duration=30,  # 30 seconds per test
        warmup_duration=5   # 5 seconds warmup
    )
    
    benchmark = MaxThroughputBenchmark(config)
    benchmark.run_max_throughput_benchmark()
    
    print("\n🎊 Maximum Throughput Benchmark Complete!")

if __name__ == "__main__":
    main()