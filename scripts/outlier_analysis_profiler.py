#!/usr/bin/env python3
"""
Outlier Analysis Profiler for CrabCache

This script specifically analyzes P99.9 and P99.99 outliers to identify
root causes and optimization opportunities.
"""

import socket
import struct
import time
import statistics
import threading
from concurrent.futures import ThreadPoolExecutor
import numpy as np
import json
import psutil
import os
from dataclasses import dataclass
from typing import List, Dict, Any, Tuple

# Binary protocol constants
CMD_PING = 0x01
CMD_PUT = 0x02
CMD_GET = 0x03

RESP_OK = 0x10
RESP_PONG = 0x11
RESP_VALUE = 0x14

@dataclass
class OutlierAnalysisConfig:
    host: str = "127.0.0.1"
    port: int = 7001
    connections: int = 5
    operations_per_connection: int = 20000  # Large sample for good outlier detection
    sample_interval_ms: int = 100  # Sample system metrics every 100ms

class SystemMetricsCollector:
    """Collect system metrics during benchmark to correlate with outliers"""
    
    def __init__(self, sample_interval_ms: int = 100):
        self.sample_interval = sample_interval_ms / 1000.0
        self.metrics = []
        self.collecting = False
        self.thread = None
    
    def start_collection(self):
        """Start collecting system metrics"""
        self.collecting = True
        self.metrics = []
        self.thread = threading.Thread(target=self._collect_metrics)
        self.thread.daemon = True
        self.thread.start()
    
    def stop_collection(self):
        """Stop collecting system metrics"""
        self.collecting = False
        if self.thread:
            self.thread.join(timeout=1.0)
    
    def _collect_metrics(self):
        """Collect system metrics in background thread"""
        while self.collecting:
            try:
                timestamp = time.perf_counter()
                
                # CPU metrics
                cpu_percent = psutil.cpu_percent(interval=None)
                cpu_freq = psutil.cpu_freq()
                
                # Memory metrics
                memory = psutil.virtual_memory()
                
                # Network metrics (if available)
                network = psutil.net_io_counters()
                
                # Process metrics for current Python process
                process = psutil.Process()
                process_info = process.as_dict(attrs=[
                    'cpu_percent', 'memory_info', 'num_threads', 
                    'num_fds', 'connections'
                ])
                
                metric = {
                    'timestamp': timestamp,
                    'cpu_percent': cpu_percent,
                    'cpu_freq_current': cpu_freq.current if cpu_freq else None,
                    'memory_percent': memory.percent,
                    'memory_available': memory.available,
                    'network_bytes_sent': network.bytes_sent,
                    'network_bytes_recv': network.bytes_recv,
                    'process_cpu_percent': process_info['cpu_percent'],
                    'process_memory_rss': process_info['memory_info'].rss,
                    'process_threads': process_info['num_threads'],
                    'process_fds': process_info.get('num_fds', 0),
                    'process_connections': len(process_info.get('connections', [])),
                }
                
                self.metrics.append(metric)
                
            except Exception as e:
                # Don't let metrics collection crash the benchmark
                pass
            
            time.sleep(self.sample_interval)
    
    def get_metrics_at_time(self, target_time: float, tolerance: float = 0.1) -> Dict:
        """Get metrics closest to target time"""
        if not self.metrics:
            return {}
        
        closest_metric = min(
            self.metrics, 
            key=lambda m: abs(m['timestamp'] - target_time)
        )
        
        if abs(closest_metric['timestamp'] - target_time) <= tolerance:
            return closest_metric
        return {}

class OutlierAnalysisClient:
    """Client that tracks detailed timing and context for outlier analysis"""
    
    def __init__(self, host: str, port: int, client_id: int):
        self.host = host
        self.port = port
        self.client_id = client_id
        self.socket = None
        self.connected = False
    
    def connect_with_monitoring(self) -> bool:
        """Connect with detailed monitoring"""
        try:
            self.socket = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
            
            # Ultra-optimized settings
            self.socket.setsockopt(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)
            self.socket.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
            self.socket.setsockopt(socket.SOL_SOCKET, socket.SO_SNDBUF, 4096)
            self.socket.setsockopt(socket.SOL_SOCKET, socket.SO_RCVBUF, 4096)
            self.socket.settimeout(5.0)
            
            connect_start = time.perf_counter_ns()
            self.socket.connect((self.host, self.port))
            connect_end = time.perf_counter_ns()
            
            connect_time_ms = (connect_end - connect_start) / 1_000_000
            
            # Test with PING
            ping_start = time.perf_counter_ns()
            self.socket.send(bytes([CMD_PING]))
            response = self.socket.recv(1)
            ping_end = time.perf_counter_ns()
            
            ping_time_ms = (ping_end - ping_start) / 1_000_000
            
            if len(response) == 1 and response[0] == RESP_PONG:
                self.connected = True
                return True, connect_time_ms, ping_time_ms
            
            return False, connect_time_ms, ping_time_ms
            
        except Exception as e:
            return False, 0, 0
    
    def disconnect(self):
        """Clean disconnect"""
        if self.socket:
            try:
                self.socket.close()
            except:
                pass
            self.socket = None
            self.connected = False
    
    def run_outlier_analysis_operations(self, operations: int) -> Dict:
        """Run operations with detailed outlier tracking"""
        results = {
            "successful_operations": 0,
            "failed_operations": 0,
            "operation_details": [],  # Detailed info for each operation
            "outliers": [],  # Operations > 2ms
            "extreme_outliers": [],  # Operations > 10ms
        }
        
        # Pre-generate commands
        ping_cmd = bytes([CMD_PING])
        
        for i in range(operations):
            operation_start = time.perf_counter_ns()
            operation_wall_start = time.perf_counter()
            
            try:
                # Send command
                send_start = time.perf_counter_ns()
                self.socket.send(ping_cmd)
                send_end = time.perf_counter_ns()
                
                # Receive response
                recv_start = time.perf_counter_ns()
                response = self.socket.recv(1)
                recv_end = time.perf_counter_ns()
                
                operation_end = time.perf_counter_ns()
                
                # Calculate detailed timings
                total_time_ns = operation_end - operation_start
                send_time_ns = send_end - send_start
                recv_time_ns = recv_end - recv_start
                network_time_ns = recv_start - send_end  # Time between send and recv
                
                total_time_ms = total_time_ns / 1_000_000
                send_time_ms = send_time_ns / 1_000_000
                recv_time_ms = recv_time_ns / 1_000_000
                network_time_ms = network_time_ns / 1_000_000
                
                success = len(response) == 1 and response[0] == RESP_PONG
                
                operation_detail = {
                    "operation_id": i,
                    "client_id": self.client_id,
                    "timestamp": operation_wall_start,
                    "success": success,
                    "total_time_ms": total_time_ms,
                    "send_time_ms": send_time_ms,
                    "recv_time_ms": recv_time_ms,
                    "network_time_ms": network_time_ms,
                }
                
                results["operation_details"].append(operation_detail)
                
                if success:
                    results["successful_operations"] += 1
                    
                    # Track outliers
                    if total_time_ms > 2.0:
                        outlier_info = operation_detail.copy()
                        outlier_info["outlier_type"] = "normal" if total_time_ms <= 10.0 else "extreme"
                        results["outliers"].append(outlier_info)
                        
                        if total_time_ms > 10.0:
                            results["extreme_outliers"].append(outlier_info)
                else:
                    results["failed_operations"] += 1
                    
            except Exception as e:
                results["failed_operations"] += 1
                results["operation_details"].append({
                    "operation_id": i,
                    "client_id": self.client_id,
                    "timestamp": operation_wall_start,
                    "success": False,
                    "error": str(e),
                    "total_time_ms": 0,
                })
        
        return results

class OutlierAnalysisProfiler:
    """Profiler to analyze P99.9 and P99.99 outliers"""
    
    def __init__(self, config: OutlierAnalysisConfig):
        self.config = config
        self.metrics_collector = SystemMetricsCollector(config.sample_interval_ms)
    
    def run_outlier_analysis(self):
        """Run comprehensive outlier analysis"""
        print("🔍 CrabCache Outlier Analysis Profiler")
        print("=" * 50)
        print(f"🎯 GOAL: Identify causes of P99.9 and P99.99 outliers")
        print(f"📊 Sample size: {self.config.connections * self.config.operations_per_connection:,} operations")
        print(f"🔬 System monitoring: Every {self.config.sample_interval_ms}ms")
        print()
        
        # Start system metrics collection
        print("🔧 Starting system metrics collection...")
        self.metrics_collector.start_collection()
        
        # Run benchmark with detailed tracking
        print("🔧 Running outlier analysis benchmark...")
        start_time = time.perf_counter()
        
        results = self.run_detailed_benchmark()
        
        end_time = time.perf_counter()
        duration = end_time - start_time
        
        # Stop metrics collection
        self.metrics_collector.stop_collection()
        
        # Analyze results
        self.analyze_outliers(results, duration, start_time)
    
    def run_detailed_benchmark(self) -> Dict:
        """Run benchmark with detailed operation tracking"""
        combined_results = {
            "successful_operations": 0,
            "failed_operations": 0,
            "operation_details": [],
            "outliers": [],
            "extreme_outliers": [],
            "connection_details": [],
        }
        
        def worker(client_id: int):
            client = OutlierAnalysisClient(self.config.host, self.config.port, client_id)
            
            # Connect with monitoring
            connected, connect_time, ping_time = client.connect_with_monitoring()
            
            connection_detail = {
                "client_id": client_id,
                "connected": connected,
                "connect_time_ms": connect_time,
                "initial_ping_time_ms": ping_time,
            }
            
            if not connected:
                return {
                    "successful_operations": 0,
                    "failed_operations": self.config.operations_per_connection,
                    "operation_details": [],
                    "outliers": [],
                    "extreme_outliers": [],
                    "connection_details": [connection_detail],
                }
            
            try:
                results = client.run_outlier_analysis_operations(self.config.operations_per_connection)
                results["connection_details"] = [connection_detail]
                client.disconnect()
                return results
            except Exception as e:
                client.disconnect()
                return {
                    "successful_operations": 0,
                    "failed_operations": self.config.operations_per_connection,
                    "operation_details": [],
                    "outliers": [],
                    "extreme_outliers": [],
                    "connection_details": [connection_detail],
                    "error": str(e),
                }
        
        # Run workers
        with ThreadPoolExecutor(max_workers=self.config.connections) as executor:
            futures = [executor.submit(worker, i) for i in range(self.config.connections)]
            
            for future in futures:
                worker_results = future.result()
                combined_results["successful_operations"] += worker_results["successful_operations"]
                combined_results["failed_operations"] += worker_results["failed_operations"]
                combined_results["operation_details"].extend(worker_results["operation_details"])
                combined_results["outliers"].extend(worker_results["outliers"])
                combined_results["extreme_outliers"].extend(worker_results["extreme_outliers"])
                combined_results["connection_details"].extend(worker_results["connection_details"])
        
        return combined_results
    
    def analyze_outliers(self, results: Dict, duration: float, start_time: float):
        """Analyze outliers and identify root causes"""
        print("🔍 Outlier Analysis Results:")
        print("=" * 40)
        
        # Basic statistics
        total_ops = results["successful_operations"] + results["failed_operations"]
        success_rate = results["successful_operations"] / total_ops * 100 if total_ops > 0 else 0
        throughput = results["successful_operations"] / duration if duration > 0 else 0
        
        print(f"Total Operations: {total_ops:,}")
        print(f"Successful Operations: {results['successful_operations']:,}")
        print(f"Success Rate: {success_rate:.1f}%")
        print(f"Throughput: {throughput:,.0f} ops/sec")
        print(f"Duration: {duration:.2f}s")
        print()
        
        # Extract latencies from successful operations
        latencies = []
        for detail in results["operation_details"]:
            if detail["success"]:
                latencies.append(detail["total_time_ms"])
        
        if not latencies:
            print("❌ No successful operations to analyze")
            return
        
        # Calculate percentiles
        latencies_array = np.array(latencies)
        percentiles = [50, 90, 95, 99, 99.9, 99.99]
        percentile_values = np.percentile(latencies_array, percentiles)
        
        print("📊 Latency Percentiles:")
        for p, value in zip(percentiles, percentile_values):
            status = "✅" if value < 1.0 or p < 99 else "⚠️" if value < 5.0 else "❌"
            print(f"  P{p:>5}: {value:>7.3f}ms {status}")
        
        print()
        
        # Outlier analysis
        outlier_count = len(results["outliers"])
        extreme_outlier_count = len(results["extreme_outliers"])
        
        print("🔍 Outlier Analysis:")
        print(f"  Normal outliers (>2ms): {outlier_count:,} ({outlier_count/len(latencies)*100:.2f}%)")
        print(f"  Extreme outliers (>10ms): {extreme_outlier_count:,} ({extreme_outlier_count/len(latencies)*100:.2f}%)")
        print()
        
        # Analyze outlier patterns
        self.analyze_outlier_patterns(results["outliers"], start_time)
        
        # Analyze timing breakdown
        self.analyze_timing_breakdown(results["operation_details"])
        
        # Correlate with system metrics
        self.correlate_with_system_metrics(results["outliers"], start_time)
        
        # Generate recommendations
        self.generate_optimization_recommendations(results, percentile_values)
        
        # Save detailed results
        self.save_outlier_analysis_results(results, duration, percentile_values)
    
    def analyze_outlier_patterns(self, outliers: List[Dict], start_time: float):
        """Analyze patterns in outliers"""
        print("🔍 Outlier Pattern Analysis:")
        print("-" * 30)
        
        if not outliers:
            print("  ✅ No outliers detected!")
            return
        
        # Time-based clustering
        outlier_times = [o["timestamp"] - start_time for o in outliers]
        
        # Find clusters of outliers (within 1 second of each other)
        clusters = []
        current_cluster = []
        
        sorted_outliers = sorted(zip(outlier_times, outliers), key=lambda x: x[0])
        
        for time_offset, outlier in sorted_outliers:
            if not current_cluster or time_offset - current_cluster[-1][0] <= 1.0:
                current_cluster.append((time_offset, outlier))
            else:
                if len(current_cluster) > 1:
                    clusters.append(current_cluster)
                current_cluster = [(time_offset, outlier)]
        
        if len(current_cluster) > 1:
            clusters.append(current_cluster)
        
        print(f"  Outlier clusters (>1 outlier within 1s): {len(clusters)}")
        
        for i, cluster in enumerate(clusters[:5]):  # Show first 5 clusters
            cluster_start = cluster[0][0]
            cluster_end = cluster[-1][0]
            cluster_duration = cluster_end - cluster_start
            avg_latency = np.mean([o[1]["total_time_ms"] for o in cluster])
            
            print(f"    Cluster {i+1}: {len(cluster)} outliers over {cluster_duration:.1f}s, avg {avg_latency:.1f}ms")
        
        # Client distribution
        client_outliers = {}
        for outlier in outliers:
            client_id = outlier["client_id"]
            client_outliers[client_id] = client_outliers.get(client_id, 0) + 1
        
        print(f"  Outliers by client: {dict(sorted(client_outliers.items()))}")
        
        # Latency distribution of outliers
        outlier_latencies = [o["total_time_ms"] for o in outliers]
        print(f"  Outlier latency range: {min(outlier_latencies):.1f}ms - {max(outlier_latencies):.1f}ms")
        print(f"  Outlier latency avg: {np.mean(outlier_latencies):.1f}ms")
        
        print()
    
    def analyze_timing_breakdown(self, operation_details: List[Dict]):
        """Analyze timing breakdown to identify bottlenecks"""
        print("🔍 Timing Breakdown Analysis:")
        print("-" * 30)
        
        successful_ops = [op for op in operation_details if op["success"]]
        
        if not successful_ops:
            print("  ❌ No successful operations to analyze")
            return
        
        # Calculate averages for each timing component
        avg_total = np.mean([op["total_time_ms"] for op in successful_ops])
        avg_send = np.mean([op["send_time_ms"] for op in successful_ops])
        avg_recv = np.mean([op["recv_time_ms"] for op in successful_ops])
        avg_network = np.mean([op["network_time_ms"] for op in successful_ops])
        
        print(f"  Average timings:")
        print(f"    Total time:   {avg_total:.3f}ms (100%)")
        print(f"    Send time:    {avg_send:.3f}ms ({avg_send/avg_total*100:.1f}%)")
        print(f"    Network time: {avg_network:.3f}ms ({avg_network/avg_total*100:.1f}%)")
        print(f"    Recv time:    {avg_recv:.3f}ms ({avg_recv/avg_total*100:.1f}%)")
        
        # Identify bottleneck
        bottleneck = max([
            ("Send", avg_send),
            ("Network", avg_network),
            ("Recv", avg_recv)
        ], key=lambda x: x[1])
        
        print(f"    Primary bottleneck: {bottleneck[0]} ({bottleneck[1]:.3f}ms)")
        
        print()
    
    def correlate_with_system_metrics(self, outliers: List[Dict], start_time: float):
        """Correlate outliers with system metrics"""
        print("🔍 System Metrics Correlation:")
        print("-" * 30)
        
        if not outliers or not self.metrics_collector.metrics:
            print("  ⚠️  Insufficient data for correlation analysis")
            return
        
        # Find system metrics for each outlier
        correlations = []
        
        for outlier in outliers[:10]:  # Analyze first 10 outliers
            outlier_time = outlier["timestamp"]
            metrics = self.metrics_collector.get_metrics_at_time(outlier_time)
            
            if metrics:
                correlations.append({
                    "outlier_latency": outlier["total_time_ms"],
                    "cpu_percent": metrics.get("cpu_percent", 0),
                    "memory_percent": metrics.get("memory_percent", 0),
                    "process_cpu_percent": metrics.get("process_cpu_percent", 0),
                    "process_threads": metrics.get("process_threads", 0),
                })
        
        if correlations:
            avg_cpu = np.mean([c["cpu_percent"] for c in correlations])
            avg_memory = np.mean([c["memory_percent"] for c in correlations])
            avg_process_cpu = np.mean([c["process_cpu_percent"] for c in correlations])
            
            print(f"  During outliers (avg of {len(correlations)} samples):")
            print(f"    System CPU: {avg_cpu:.1f}%")
            print(f"    Memory usage: {avg_memory:.1f}%")
            print(f"    Process CPU: {avg_process_cpu:.1f}%")
            
            # Compare with overall averages
            if len(self.metrics_collector.metrics) > 10:
                overall_cpu = np.mean([m["cpu_percent"] for m in self.metrics_collector.metrics])
                overall_memory = np.mean([m["memory_percent"] for m in self.metrics_collector.metrics])
                
                print(f"  Overall averages:")
                print(f"    System CPU: {overall_cpu:.1f}%")
                print(f"    Memory usage: {overall_memory:.1f}%")
                
                if avg_cpu > overall_cpu * 1.2:
                    print(f"  🔍 CPU spike correlation detected! ({avg_cpu:.1f}% vs {overall_cpu:.1f}%)")
                if avg_memory > overall_memory * 1.1:
                    print(f"  🔍 Memory pressure correlation detected! ({avg_memory:.1f}% vs {overall_memory:.1f}%)")
        
        print()
    
    def generate_optimization_recommendations(self, results: Dict, percentiles: np.ndarray):
        """Generate specific optimization recommendations"""
        print("💡 Optimization Recommendations:")
        print("-" * 35)
        
        p99_9 = percentiles[4]  # P99.9
        p99_99 = percentiles[5]  # P99.99
        
        outlier_count = len(results["outliers"])
        extreme_outlier_count = len(results["extreme_outliers"])
        
        recommendations = []
        
        # P99.9 recommendations
        if p99_9 > 2.0:
            recommendations.append("🔧 P99.9 > 2ms: Implement connection pre-warming")
            recommendations.append("🔧 Consider CPU affinity for server process")
            recommendations.append("🔧 Tune garbage collection settings")
        
        # P99.99 recommendations
        if p99_99 > 10.0:
            recommendations.append("🔧 P99.99 > 10ms: Investigate system-level interrupts")
            recommendations.append("🔧 Consider real-time kernel or process priority")
            recommendations.append("🔧 Monitor for background system tasks")
        
        # Outlier-based recommendations
        if outlier_count > 100:
            recommendations.append("🔧 High outlier count: Review memory allocation patterns")
            recommendations.append("🔧 Consider using memory pools")
        
        if extreme_outlier_count > 10:
            recommendations.append("🔧 Extreme outliers detected: Check for OS-level issues")
            recommendations.append("🔧 Monitor system logs during benchmark")
        
        # Connection-based recommendations
        failed_connections = sum(1 for c in results["connection_details"] if not c["connected"])
        if failed_connections > 0:
            recommendations.append("🔧 Connection failures detected: Review connection limits")
        
        if not recommendations:
            recommendations.append("✅ Performance is excellent! Minor optimizations:")
            recommendations.append("🔧 Consider reducing connection pool size for even lower latency")
            recommendations.append("🔧 Experiment with different TCP buffer sizes")
        
        for rec in recommendations:
            print(f"  {rec}")
        
        print()
    
    def save_outlier_analysis_results(self, results: Dict, duration: float, percentiles: np.ndarray):
        """Save detailed outlier analysis results"""
        timestamp = time.strftime("%Y%m%d_%H%M%S")
        filename = f"crabcache/benchmark_results/outlier_analysis_{timestamp}.json"
        
        # Prepare data for JSON serialization
        json_data = {
            "timestamp": timestamp,
            "config": {
                "connections": self.config.connections,
                "operations_per_connection": self.config.operations_per_connection,
                "sample_interval_ms": self.config.sample_interval_ms,
            },
            "summary": {
                "total_operations": results["successful_operations"] + results["failed_operations"],
                "successful_operations": results["successful_operations"],
                "failed_operations": results["failed_operations"],
                "duration_seconds": duration,
                "throughput_ops_per_sec": results["successful_operations"] / duration if duration > 0 else 0,
            },
            "percentiles": {
                "p50": float(percentiles[0]),
                "p90": float(percentiles[1]),
                "p95": float(percentiles[2]),
                "p99": float(percentiles[3]),
                "p99_9": float(percentiles[4]),
                "p99_99": float(percentiles[5]),
            },
            "outlier_analysis": {
                "normal_outliers_count": len(results["outliers"]),
                "extreme_outliers_count": len(results["extreme_outliers"]),
                "outlier_percentage": len(results["outliers"]) / results["successful_operations"] * 100 if results["successful_operations"] > 0 else 0,
            },
            "system_metrics_samples": len(self.metrics_collector.metrics),
            # Note: Not including raw operation details to keep file size manageable
        }
        
        with open(filename, 'w') as f:
            json.dump(json_data, f, indent=2)
        
        print(f"💾 Outlier analysis results saved to: {filename}")

def main():
    print("🔍 Starting CrabCache Outlier Analysis Profiler...")
    print("🎯 Goal: Identify and analyze P99.9 and P99.99 outliers")
    print()
    
    config = OutlierAnalysisConfig(
        host="127.0.0.1",
        port=7001,
        connections=5,
        operations_per_connection=20000,  # Large sample for good outlier detection
        sample_interval_ms=100
    )
    
    profiler = OutlierAnalysisProfiler(config)
    profiler.run_outlier_analysis()
    
    print("\n🎊 Outlier Analysis Complete!")
    print("Check the generated report for optimization recommendations.")

if __name__ == "__main__":
    main()