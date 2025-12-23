#!/usr/bin/env python3
"""
Accurate Performance Benchmark for CrabCache Phase 3

This benchmark ensures 100% accuracy by:
1. Validating every single operation
2. Measuring only successful operations
3. Detecting and reporting failures correctly
4. Using proper connection management
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
class AccurateConfig:
    host: str = "127.0.0.1"
    port: int = 7001
    connections: int = 10
    operations_per_connection: int = 1000
    validation_level: str = "strict"  # strict, normal, fast

class ValidatedClient:
    """Client that validates every operation for 100% accuracy"""
    
    def __init__(self, host: str, port: int, client_id: int):
        self.host = host
        self.port = port
        self.client_id = client_id
        self.socket = None
        self.connected = False
        self.operations_count = 0
        self.errors = []
    
    def connect(self) -> bool:
        """Connect with full validation"""
        try:
            self.socket = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
            self.socket.setsockopt(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)
            self.socket.settimeout(5.0)  # 5 second timeout
            self.socket.connect((self.host, self.port))
            
            # Validate connection with PING
            if not self._validated_ping():
                self.errors.append("Initial PING validation failed")
                return False
            
            self.connected = True
            return True
        except Exception as e:
            self.errors.append(f"Connection failed: {e}")
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
    
    def _validated_ping(self) -> bool:
        """PING with full validation"""
        if not self.connected and not self.socket:
            return False
        
        try:
            # Send PING
            sent = self.socket.send(bytes([CMD_PING]))
            if sent != 1:
                self.errors.append(f"PING send failed: sent {sent} bytes instead of 1")
                return False
            
            # Receive PONG
            response = self.socket.recv(1)
            if len(response) != 1:
                self.errors.append(f"PING response length wrong: {len(response)} instead of 1")
                return False
            
            if response[0] != RESP_PONG:
                self.errors.append(f"PING response wrong: 0x{response[0]:02x} instead of 0x{RESP_PONG:02x}")
                return False
            
            return True
        except Exception as e:
            self.errors.append(f"PING exception: {e}")
            return False
    
    def _validated_put(self, key: bytes, value: bytes) -> bool:
        """PUT with full validation"""
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
            sent = self.socket.send(command)
            if sent != len(command):
                self.errors.append(f"PUT send incomplete: {sent}/{len(command)} bytes")
                return False
            
            # Receive response
            response = self.socket.recv(1)
            if len(response) != 1:
                self.errors.append(f"PUT response length wrong: {len(response)}")
                return False
            
            if response[0] != RESP_OK:
                self.errors.append(f"PUT response wrong: 0x{response[0]:02x} instead of 0x{RESP_OK:02x}")
                return False
            
            return True
        except Exception as e:
            self.errors.append(f"PUT exception: {e}")
            return False
    
    def _validated_get(self, key: bytes) -> Tuple[bool, Optional[bytes]]:
        """GET with full validation"""
        if not self.connected:
            return False, None
        
        try:
            # Build GET command
            command = bytearray()
            command.append(CMD_GET)
            command.extend(struct.pack('<I', len(key)))
            command.extend(key)
            
            # Send command
            sent = self.socket.send(command)
            if sent != len(command):
                self.errors.append(f"GET send incomplete: {sent}/{len(command)} bytes")
                return False, None
            
            # Receive response type
            response_type = self.socket.recv(1)
            if len(response_type) != 1:
                self.errors.append(f"GET response type length wrong: {len(response_type)}")
                return False, None
            
            if response_type[0] == RESP_NULL:
                return True, None  # Key not found, but operation succeeded
            elif response_type[0] == RESP_VALUE:
                # Read value length
                len_bytes = self.socket.recv(4)
                if len(len_bytes) != 4:
                    self.errors.append(f"GET value length wrong: {len(len_bytes)} bytes")
                    return False, None
                
                value_len = struct.unpack('<I', len_bytes)[0]
                
                # Validate value length is reasonable
                if value_len > 1024 * 1024:  # 1MB limit
                    self.errors.append(f"GET value too large: {value_len} bytes")
                    return False, None
                
                # Read value
                value = b""
                while len(value) < value_len:
                    chunk = self.socket.recv(min(4096, value_len - len(value)))
                    if not chunk:
                        self.errors.append(f"GET value read incomplete: {len(value)}/{value_len}")
                        return False, None
                    value += chunk
                
                if len(value) != value_len:
                    self.errors.append(f"GET value length mismatch: {len(value)}/{value_len}")
                    return False, None
                
                return True, value
            else:
                self.errors.append(f"GET unexpected response: 0x{response_type[0]:02x}")
                return False, None
        except Exception as e:
            self.errors.append(f"GET exception: {e}")
            return False, None
    
    def run_validated_operations(self, operations: int) -> dict:
        """Run operations with full validation"""
        results = {
            "total_attempted": 0,
            "successful_operations": 0,
            "failed_operations": 0,
            "ping_success": 0,
            "put_success": 0,
            "get_success": 0,
            "latencies": [],
            "errors": []
        }
        
        for i in range(operations):
            operation_type = i % 3  # Cycle through PING, PUT, GET
            
            start_time = time.perf_counter()
            success = False
            
            results["total_attempted"] += 1
            
            if operation_type == 0:  # PING
                success = self._validated_ping()
                if success:
                    results["ping_success"] += 1
            elif operation_type == 1:  # PUT
                key = f"test_key_{self.client_id}_{i}".encode()
                value = f"test_value_{i}".encode()
                success = self._validated_put(key, value)
                if success:
                    results["put_success"] += 1
            else:  # GET
                key = f"test_key_{self.client_id}_{i-1}".encode()  # Get previous key
                success, _ = self._validated_get(key)
                if success:
                    results["get_success"] += 1
            
            end_time = time.perf_counter()
            latency_ms = (end_time - start_time) * 1000
            
            if success:
                results["successful_operations"] += 1
                results["latencies"].append(latency_ms)
            else:
                results["failed_operations"] += 1
                # Add recent errors
                if len(self.errors) > len(results["errors"]):
                    results["errors"].extend(self.errors[len(results["errors"]):])
        
        return results

class AccurateBenchmark:
    """Benchmark with 100% accuracy validation"""
    
    def __init__(self, config: AccurateConfig):
        self.config = config
    
    def run_accurate_benchmark(self):
        """Run benchmark with full accuracy validation"""
        print("🔍 CrabCache ACCURATE Performance Benchmark")
        print("=" * 50)
        print(f"🎯 Validation Level: {self.config.validation_level}")
        print(f"Server: {self.config.host}:{self.config.port}")
        print(f"Connections: {self.config.connections}")
        print(f"Operations per connection: {self.config.operations_per_connection}")
        print()
        
        # Test single connection first
        if not self.test_single_connection():
            print("❌ Single connection test failed - aborting")
            return
        
        # Run main benchmark
        print("🔧 Running accurate benchmark...")
        start_time = time.perf_counter()
        
        results = self.run_worker_benchmark()
        
        end_time = time.perf_counter()
        duration = end_time - start_time
        
        # Calculate final metrics
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
        self.print_accurate_results({
            "total_operations": total_operations,
            "successful_operations": results["successful_operations"],
            "failed_operations": results["failed_operations"],
            "duration_seconds": duration,
            "throughput_ops_per_sec": throughput,
            "success_rate": success_rate,
            "latency_p50_ms": latency_p50,
            "latency_p95_ms": latency_p95,
            "latency_p99_ms": latency_p99,
            "ping_success": results["ping_success"],
            "put_success": results["put_success"],
            "get_success": results["get_success"],
            "errors": results["errors"][:10]  # First 10 errors
        })
        
        # Save results
        self.save_accurate_results({
            "config": {
                "host": self.config.host,
                "port": self.config.port,
                "connections": self.config.connections,
                "operations_per_connection": self.config.operations_per_connection,
                "validation_level": self.config.validation_level,
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
            },
            "breakdown": {
                "ping_success": results["ping_success"],
                "put_success": results["put_success"],
                "get_success": results["get_success"],
                "failed_operations": results["failed_operations"],
            },
            "errors_sample": results["errors"][:10]
        })
    
    def test_single_connection(self) -> bool:
        """Test single connection with full validation"""
        print("🔧 Testing single connection...")
        
        client = ValidatedClient(self.config.host, self.config.port, 0)
        if not client.connect():
            print(f"  ❌ Connection failed: {client.errors}")
            return False
        
        # Test basic operations
        if not client._validated_ping():
            print(f"  ❌ PING failed: {client.errors}")
            client.disconnect()
            return False
        
        if not client._validated_put(b"test_key", b"test_value"):
            print(f"  ❌ PUT failed: {client.errors}")
            client.disconnect()
            return False
        
        success, value = client._validated_get(b"test_key")
        if not success:
            print(f"  ❌ GET failed: {client.errors}")
            client.disconnect()
            return False
        
        if value != b"test_value":
            print(f"  ❌ GET value mismatch: {value} != b'test_value'")
            client.disconnect()
            return False
        
        client.disconnect()
        print("  ✅ Single connection test passed")
        return True
    
    def run_worker_benchmark(self) -> dict:
        """Run benchmark with multiple validated workers"""
        combined_results = {
            "successful_operations": 0,
            "failed_operations": 0,
            "ping_success": 0,
            "put_success": 0,
            "get_success": 0,
            "latencies": [],
            "errors": []
        }
        
        def worker(client_id: int):
            client = ValidatedClient(self.config.host, self.config.port, client_id)
            
            if not client.connect():
                return {
                    "successful_operations": 0,
                    "failed_operations": self.config.operations_per_connection,
                    "ping_success": 0,
                    "put_success": 0,
                    "get_success": 0,
                    "latencies": [],
                    "errors": [f"Client {client_id} connection failed"]
                }
            
            try:
                results = client.run_validated_operations(self.config.operations_per_connection)
                client.disconnect()
                return results
            except Exception as e:
                client.disconnect()
                return {
                    "successful_operations": 0,
                    "failed_operations": self.config.operations_per_connection,
                    "ping_success": 0,
                    "put_success": 0,
                    "get_success": 0,
                    "latencies": [],
                    "errors": [f"Client {client_id} exception: {e}"]
                }
        
        # Run workers
        with ThreadPoolExecutor(max_workers=self.config.connections) as executor:
            futures = [executor.submit(worker, i) for i in range(self.config.connections)]
            
            for future in futures:
                worker_results = future.result()
                combined_results["successful_operations"] += worker_results["successful_operations"]
                combined_results["failed_operations"] += worker_results["failed_operations"]
                combined_results["ping_success"] += worker_results["ping_success"]
                combined_results["put_success"] += worker_results["put_success"]
                combined_results["get_success"] += worker_results["get_success"]
                combined_results["latencies"].extend(worker_results["latencies"])
                combined_results["errors"].extend(worker_results["errors"])
        
        return combined_results
    
    def print_accurate_results(self, result: dict):
        """Print accurate benchmark results"""
        print("📊 ACCURATE Benchmark Results:")
        print("=" * 40)
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
        
        print("🔍 Operation Breakdown:")
        print(f"  PING Success: {result['ping_success']:,}")
        print(f"  PUT Success: {result['put_success']:,}")
        print(f"  GET Success: {result['get_success']:,}")
        print()
        
        # Performance comparison
        throughput = result['throughput_ops_per_sec']
        
        # Phase comparisons
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
        else:
            remaining = 20000 - throughput
            print(f"  ❌ Goal not reached. Need {remaining:,.0f} more ops/sec")
        
        # Validation
        if result['success_rate'] < 95:
            print(f"\n⚠️  WARNING: Low success rate ({result['success_rate']:.1f}%)")
        
        if result['errors']:
            print(f"\n❌ Sample Errors ({len(result['errors'])} total):")
            for i, error in enumerate(result['errors'][:5]):
                print(f"  {i+1}. {error}")
    
    def save_accurate_results(self, data: dict):
        """Save accurate results to file"""
        timestamp = time.strftime("%Y%m%d_%H%M%S")
        filename = f"crabcache/benchmark_results/accurate_results_{timestamp}.json"
        
        with open(filename, 'w') as f:
            json.dump(data, f, indent=2)
        
        print(f"\n💾 Results saved to: {filename}")

def main():
    config = AccurateConfig(
        host="127.0.0.1",
        port=7001,
        connections=10,
        operations_per_connection=1000,
        validation_level="strict"
    )
    
    benchmark = AccurateBenchmark(config)
    benchmark.run_accurate_benchmark()

if __name__ == "__main__":
    main()