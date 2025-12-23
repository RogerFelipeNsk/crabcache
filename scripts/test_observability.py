#!/usr/bin/env python3
"""
Test script for CrabCache observability features
Tests metrics endpoint, dashboard, and STATS command
"""

import asyncio
import aiohttp
import json
import time
import sys
from typing import Dict, Any

class ObservabilityTester:
    def __init__(self, tcp_host='localhost', tcp_port=7001, metrics_port=9090):
        self.tcp_host = tcp_host
        self.tcp_port = tcp_port
        self.metrics_port = metrics_port
        self.base_url = f"http://{tcp_host}:{metrics_port}"
        
    async def test_tcp_connection(self) -> bool:
        """Test basic TCP connectivity"""
        try:
            reader, writer = await asyncio.open_connection(self.tcp_host, self.tcp_port)
            
            # Send PING command
            writer.write(b"PING\r\n")
            await writer.drain()
            
            # Read response
            response = await reader.readline()
            writer.close()
            await writer.wait_closed()
            
            return response.strip() == b"PONG"
        except Exception as e:
            print(f"❌ TCP connection failed: {e}")
            return False
    
    async def test_stats_command(self) -> Dict[str, Any]:
        """Test STATS command and parse JSON response"""
        try:
            reader, writer = await asyncio.open_connection(self.tcp_host, self.tcp_port)
            
            # Send STATS command
            writer.write(b"STATS\r\n")
            await writer.drain()
            
            # Read response
            response = await reader.readline()
            writer.close()
            await writer.wait_closed()
            
            # Parse JSON response
            response_str = response.decode('utf-8').strip()
            if response_str.startswith('+'):
                # Remove Redis-style prefix
                response_str = response_str[1:]
            
            stats = json.loads(response_str)
            return stats
        except Exception as e:
            print(f"❌ STATS command failed: {e}")
            return {}
    
    async def test_metrics_endpoint(self) -> str:
        """Test Prometheus metrics endpoint"""
        try:
            async with aiohttp.ClientSession() as session:
                async with session.get(f"{self.base_url}/metrics") as response:
                    if response.status == 200:
                        content = await response.text()
                        return content
                    else:
                        print(f"❌ Metrics endpoint returned status {response.status}")
                        return ""
        except Exception as e:
            print(f"❌ Metrics endpoint failed: {e}")
            return ""
    
    async def test_dashboard_endpoint(self) -> bool:
        """Test dashboard endpoint"""
        try:
            async with aiohttp.ClientSession() as session:
                async with session.get(f"{self.base_url}/dashboard") as response:
                    if response.status == 200:
                        content = await response.text()
                        return "CrabCache Dashboard" in content
                    else:
                        print(f"❌ Dashboard endpoint returned status {response.status}")
                        return False
        except Exception as e:
            print(f"❌ Dashboard endpoint failed: {e}")
            return False
    
    async def test_health_endpoint(self) -> Dict[str, Any]:
        """Test health check endpoint"""
        try:
            async with aiohttp.ClientSession() as session:
                async with session.get(f"{self.base_url}/health") as response:
                    if response.status == 200:
                        health = await response.json()
                        return health
                    else:
                        print(f"❌ Health endpoint returned status {response.status}")
                        return {}
        except Exception as e:
            print(f"❌ Health endpoint failed: {e}")
            return {}
    
    async def generate_load(self, operations=100) -> None:
        """Generate some load to populate metrics"""
        print(f"🔄 Generating {operations} operations for metrics...")
        
        try:
            reader, writer = await asyncio.open_connection(self.tcp_host, self.tcp_port)
            
            for i in range(operations):
                # Mix of operations
                if i % 4 == 0:
                    writer.write(f"PUT key_{i} value_{i}\r\n".encode())
                elif i % 4 == 1:
                    writer.write(f"GET key_{i//4}\r\n".encode())
                elif i % 4 == 2:
                    writer.write(f"DEL key_{i//4}\r\n".encode())
                else:
                    writer.write(b"PING\r\n")
                
                await writer.drain()
                
                # Read response
                await reader.readline()
                
                # Small delay to spread operations
                if i % 10 == 0:
                    await asyncio.sleep(0.01)
            
            writer.close()
            await writer.wait_closed()
            
        except Exception as e:
            print(f"❌ Load generation failed: {e}")
    
    def analyze_stats(self, stats: Dict[str, Any]) -> None:
        """Analyze and display stats"""
        if not stats:
            return
        
        print("\n📊 STATS Analysis:")
        
        # Global metrics
        if 'global' in stats:
            global_stats = stats['global']
            print(f"  Uptime: {global_stats.get('uptime_seconds', 0)} seconds")
            print(f"  Total Operations: {global_stats.get('total_operations', 0):,}")
            print(f"  Operations/sec: {global_stats.get('operations_per_second', 0):.1f}")
            print(f"  Cache Hit Ratio: {global_stats.get('cache_hit_ratio', 0)*100:.1f}%")
            print(f"  Memory Used: {global_stats.get('memory_used_bytes', 0)/1024/1024:.1f} MB")
            print(f"  Active Connections: {global_stats.get('active_connections', 0)}")
        
        # Latency metrics
        if 'latency' in stats:
            latency = stats['latency']
            print(f"\n⚡ Latency Metrics:")
            print(f"  P50: {latency.get('p50_ms', 0):.3f}ms")
            print(f"  P95: {latency.get('p95_ms', 0):.3f}ms")
            print(f"  P99: {latency.get('p99_ms', 0):.3f}ms")
            print(f"  P99.9: {latency.get('p99_9_ms', 0):.3f}ms")
            print(f"  Mean: {latency.get('mean_ms', 0):.3f}ms")
        
        # Shard metrics
        if 'shards' in stats:
            print(f"\n🔧 Shard Metrics ({len(stats['shards'])} shards):")
            for i, shard in enumerate(stats['shards'][:3]):  # Show first 3 shards
                print(f"  Shard {i}: {shard.get('operations', 0)} ops, {shard.get('items', 0)} items")
    
    def analyze_prometheus_metrics(self, metrics: str) -> None:
        """Analyze Prometheus metrics"""
        if not metrics:
            return
        
        print("\n📈 Prometheus Metrics Analysis:")
        
        lines = metrics.split('\n')
        metric_count = 0
        
        for line in lines:
            if line.startswith('crabcache_') and not line.startswith('#'):
                metric_count += 1
                if metric_count <= 5:  # Show first 5 metrics
                    print(f"  {line}")
        
        print(f"  ... and {metric_count - 5} more metrics" if metric_count > 5 else "")
        print(f"  Total metrics: {metric_count}")
    
    async def run_full_test(self) -> bool:
        """Run complete observability test suite"""
        print("🧪 CrabCache Observability Test Suite")
        print("=" * 50)
        
        # Test 1: TCP Connection
        print("\n1️⃣ Testing TCP Connection...")
        tcp_ok = await self.test_tcp_connection()
        if tcp_ok:
            print("✅ TCP connection successful")
        else:
            print("❌ TCP connection failed")
            return False
        
        # Test 2: Generate some load
        await self.generate_load(50)
        
        # Wait a moment for metrics to update
        await asyncio.sleep(1)
        
        # Test 3: STATS Command
        print("\n2️⃣ Testing STATS Command...")
        stats = await self.test_stats_command()
        if stats:
            print("✅ STATS command successful")
            self.analyze_stats(stats)
        else:
            print("❌ STATS command failed")
        
        # Test 4: Metrics Endpoint
        print("\n3️⃣ Testing Prometheus Metrics Endpoint...")
        metrics = await self.test_metrics_endpoint()
        if metrics:
            print("✅ Metrics endpoint successful")
            self.analyze_prometheus_metrics(metrics)
        else:
            print("❌ Metrics endpoint failed")
        
        # Test 5: Dashboard
        print("\n4️⃣ Testing Dashboard Endpoint...")
        dashboard_ok = await self.test_dashboard_endpoint()
        if dashboard_ok:
            print("✅ Dashboard endpoint successful")
        else:
            print("❌ Dashboard endpoint failed")
        
        # Test 6: Health Check
        print("\n5️⃣ Testing Health Endpoint...")
        health = await self.test_health_endpoint()
        if health:
            print("✅ Health endpoint successful")
            print(f"  Status: {health.get('status', 'unknown')}")
            print(f"  Service: {health.get('service', 'unknown')}")
        else:
            print("❌ Health endpoint failed")
        
        # Summary
        print("\n" + "=" * 50)
        print("🎯 Test Summary:")
        print(f"  TCP Connection: {'✅' if tcp_ok else '❌'}")
        print(f"  STATS Command: {'✅' if stats else '❌'}")
        print(f"  Metrics Endpoint: {'✅' if metrics else '❌'}")
        print(f"  Dashboard: {'✅' if dashboard_ok else '❌'}")
        print(f"  Health Check: {'✅' if health else '❌'}")
        
        all_passed = tcp_ok and bool(stats) and bool(metrics) and dashboard_ok and bool(health)
        
        if all_passed:
            print("\n🎉 All observability tests PASSED!")
            print(f"📊 Dashboard: http://localhost:{self.metrics_port}/dashboard")
            print(f"📈 Metrics: http://localhost:{self.metrics_port}/metrics")
        else:
            print("\n❌ Some tests FAILED!")
        
        return all_passed

async def main():
    if len(sys.argv) > 1:
        if sys.argv[1] == "--help":
            print("Usage: python test_observability.py [tcp_port] [metrics_port]")
            print("Default: tcp_port=7001, metrics_port=9090")
            return
    
    tcp_port = int(sys.argv[1]) if len(sys.argv) > 1 else 7001
    metrics_port = int(sys.argv[2]) if len(sys.argv) > 2 else 9090
    
    tester = ObservabilityTester(tcp_port=tcp_port, metrics_port=metrics_port)
    
    success = await tester.run_full_test()
    
    if not success:
        sys.exit(1)

if __name__ == "__main__":
    asyncio.run(main())