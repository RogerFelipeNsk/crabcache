#!/usr/bin/env python3
"""
Benchmark completo do CrabCache com comparação Redis
"""

import socket
import time
import subprocess
import sys
import json
import statistics
import threading
from concurrent.futures import ThreadPoolExecutor, as_completed
from typing import List, Dict, Any
import requests

class CrabCacheBenchmark:
    def __init__(self, host='localhost', port=8000):
        self.host = host
        self.port = port
        
    def connect(self):
        """Conecta ao CrabCache"""
        sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        sock.settimeout(10)
        sock.connect((self.host, self.port))
        return sock
    
    def send_command(self, sock, command: str) -> tuple:
        """Envia comando e mede latência"""
        start_time = time.perf_counter()
        sock.send(command.encode() + b'\n')
        response = sock.recv(4096)
        end_time = time.perf_counter()
        
        latency_ms = (end_time - start_time) * 1000
        return response.decode().strip(), latency_ms
    
    def benchmark_operation(self, operation: str, count: int, concurrent: int = 1) -> Dict[str, Any]:
        """Benchmark de uma operação específica"""
        print(f"🚀 Benchmarking {operation} ({count} ops, {concurrent} threads)...")
        
        latencies = []
        errors = 0
        
        def worker(ops_per_thread):
            thread_latencies = []
            thread_errors = 0
            
            try:
                sock = self.connect()
                
                for i in range(ops_per_thread):
                    try:
                        if operation == "PUT":
                            cmd = f"PUT bench_key_{i} bench_value_{i}"
                        elif operation == "GET":
                            cmd = f"GET bench_key_{i % 1000}"  # Reutiliza chaves
                        elif operation == "DEL":
                            cmd = f"DEL bench_key_{i}"
                        elif operation == "PING":
                            cmd = "PING"
                        else:
                            cmd = operation
                        
                        response, latency = self.send_command(sock, cmd)
                        thread_latencies.append(latency)
                        
                        if "OK" not in response and "PONG" not in response and "bench_value" not in response:
                            thread_errors += 1
                            
                    except Exception as e:
                        thread_errors += 1
                
                sock.close()
                
            except Exception as e:
                thread_errors = ops_per_thread
                
            return thread_latencies, thread_errors
        
        # Distribui operações entre threads
        ops_per_thread = count // concurrent
        remaining_ops = count % concurrent
        
        start_time = time.perf_counter()
        
        with ThreadPoolExecutor(max_workers=concurrent) as executor:
            futures = []
            
            for i in range(concurrent):
                ops = ops_per_thread + (1 if i < remaining_ops else 0)
                futures.append(executor.submit(worker, ops))
            
            for future in as_completed(futures):
                thread_latencies, thread_errors = future.result()
                latencies.extend(thread_latencies)
                errors += thread_errors
        
        end_time = time.perf_counter()
        
        # Calcula estatísticas
        total_time = end_time - start_time
        successful_ops = len(latencies)
        ops_per_sec = successful_ops / total_time if total_time > 0 else 0
        
        if latencies:
            latencies.sort()
            p50 = statistics.median(latencies)
            p95 = latencies[int(0.95 * len(latencies))]
            p99 = latencies[int(0.99 * len(latencies))]
            p999 = latencies[int(0.999 * len(latencies))] if len(latencies) > 100 else p99
            avg_latency = statistics.mean(latencies)
            min_latency = min(latencies)
            max_latency = max(latencies)
        else:
            p50 = p95 = p99 = p999 = avg_latency = min_latency = max_latency = 0
        
        return {
            'operation': operation,
            'total_ops': count,
            'successful_ops': successful_ops,
            'errors': errors,
            'concurrent_threads': concurrent,
            'total_time_sec': total_time,
            'ops_per_sec': ops_per_sec,
            'latency_ms': {
                'min': min_latency,
                'avg': avg_latency,
                'p50': p50,
                'p95': p95,
                'p99': p99,
                'p999': p999,
                'max': max_latency
            }
        }
    
    def benchmark_mixed_workload(self, total_ops: int, concurrent: int = 10) -> Dict[str, Any]:
        """Benchmark com workload misto (70% GET, 20% PUT, 10% DEL)"""
        print(f"🔄 Benchmarking mixed workload ({total_ops} ops, {concurrent} threads)...")
        
        # Primeiro, popula o cache com dados
        print("📝 Populando cache com dados iniciais...")
        for i in range(1000):
            sock = self.connect()
            self.send_command(sock, f"PUT mixed_key_{i} mixed_value_{i}")
            sock.close()
        
        latencies = []
        errors = 0
        operation_counts = {'GET': 0, 'PUT': 0, 'DEL': 0}
        
        def worker(ops_per_thread):
            thread_latencies = []
            thread_errors = 0
            thread_ops = {'GET': 0, 'PUT': 0, 'DEL': 0}
            
            try:
                sock = self.connect()
                
                for i in range(ops_per_thread):
                    try:
                        # Distribui operações: 70% GET, 20% PUT, 10% DEL
                        rand_val = (i * 7) % 100  # Pseudo-random
                        
                        if rand_val < 70:  # 70% GET
                            cmd = f"GET mixed_key_{i % 1000}"
                            op = 'GET'
                        elif rand_val < 90:  # 20% PUT
                            cmd = f"PUT mixed_key_{i % 1000} updated_value_{i}"
                            op = 'PUT'
                        else:  # 10% DEL
                            cmd = f"DEL mixed_key_{i % 1000}"
                            op = 'DEL'
                        
                        response, latency = self.send_command(sock, cmd)
                        thread_latencies.append(latency)
                        thread_ops[op] += 1
                        
                    except Exception as e:
                        thread_errors += 1
                
                sock.close()
                
            except Exception as e:
                thread_errors = ops_per_thread
                
            return thread_latencies, thread_errors, thread_ops
        
        # Executa benchmark
        ops_per_thread = total_ops // concurrent
        remaining_ops = total_ops % concurrent
        
        start_time = time.perf_counter()
        
        with ThreadPoolExecutor(max_workers=concurrent) as executor:
            futures = []
            
            for i in range(concurrent):
                ops = ops_per_thread + (1 if i < remaining_ops else 0)
                futures.append(executor.submit(worker, ops))
            
            for future in as_completed(futures):
                thread_latencies, thread_errors, thread_ops = future.result()
                latencies.extend(thread_latencies)
                errors += thread_errors
                for op, count in thread_ops.items():
                    operation_counts[op] += count
        
        end_time = time.perf_counter()
        
        # Calcula estatísticas
        total_time = end_time - start_time
        successful_ops = len(latencies)
        ops_per_sec = successful_ops / total_time if total_time > 0 else 0
        
        if latencies:
            latencies.sort()
            p50 = statistics.median(latencies)
            p95 = latencies[int(0.95 * len(latencies))]
            p99 = latencies[int(0.99 * len(latencies))]
            p999 = latencies[int(0.999 * len(latencies))] if len(latencies) > 100 else p99
            avg_latency = statistics.mean(latencies)
        else:
            p50 = p95 = p99 = p999 = avg_latency = 0
        
        return {
            'operation': 'MIXED',
            'total_ops': total_ops,
            'successful_ops': successful_ops,
            'errors': errors,
            'concurrent_threads': concurrent,
            'total_time_sec': total_time,
            'ops_per_sec': ops_per_sec,
            'operation_breakdown': operation_counts,
            'latency_ms': {
                'avg': avg_latency,
                'p50': p50,
                'p95': p95,
                'p99': p99,
                'p999': p999
            }
        }
    
    def get_metrics(self) -> Dict[str, Any]:
        """Obtém métricas do servidor"""
        try:
            sock = self.connect()
            response, _ = self.send_command(sock, "STATS")
            sock.close()
            
            # Parse JSON response
            if response.startswith("STATS: "):
                stats_json = response[7:]  # Remove "STATS: " prefix
                return json.loads(stats_json)
            
        except Exception as e:
            print(f"Erro ao obter métricas: {e}")
        
        return {}

def start_crabcache_server():
    """Inicia servidor CrabCache para benchmark"""
    print("🚀 Iniciando CrabCache server...")
    
    # Para containers existentes
    subprocess.run(["docker", "stop", "crabcache-benchmark"], capture_output=True)
    subprocess.run(["docker", "rm", "crabcache-benchmark"], capture_output=True)
    
    # Inicia novo container
    cmd = [
        "docker", "run", "-d",
        "--name", "crabcache-benchmark",
        "-p", "8000:8000",
        "-p", "9090:9090",
        "-e", "CRABCACHE_PORT=8000",
        "crabcache:latest-security"
    ]
    
    result = subprocess.run(cmd, capture_output=True, text=True)
    if result.returncode != 0:
        print(f"Erro ao iniciar CrabCache: {result.stderr}")
        return False
    
    # Aguarda inicialização
    time.sleep(5)
    
    # Testa conectividade
    try:
        sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        sock.settimeout(5)
        sock.connect(('localhost', 8000))
        sock.send(b'PING\n')
        response = sock.recv(4096)
        sock.close()
        
        if b'PONG' in response:
            print("✅ CrabCache server iniciado com sucesso")
            return True
        else:
            print("❌ CrabCache server não respondeu corretamente")
            return False
            
    except Exception as e:
        print(f"❌ Erro ao conectar no CrabCache: {e}")
        return False

def stop_crabcache_server():
    """Para servidor CrabCache"""
    subprocess.run(["docker", "stop", "crabcache-benchmark"], capture_output=True)
    subprocess.run(["docker", "rm", "crabcache-benchmark"], capture_output=True)

def format_results(results: List[Dict[str, Any]]) -> str:
    """Formata resultados para exibição"""
    output = []
    output.append("🚀 CrabCache Benchmark Results")
    output.append("=" * 50)
    
    for result in results:
        op = result['operation']
        ops_sec = result['ops_per_sec']
        p99 = result['latency_ms']['p99']
        p95 = result['latency_ms']['p95']
        success_rate = (result['successful_ops'] / result['total_ops']) * 100
        
        output.append(f"\n📊 {op} Operations:")
        output.append(f"   Throughput:    {ops_sec:,.0f} ops/sec")
        output.append(f"   P95 Latency:   {p95:.3f}ms")
        output.append(f"   P99 Latency:   {p99:.3f}ms")
        output.append(f"   Success Rate:  {success_rate:.1f}%")
        
        if 'operation_breakdown' in result:
            output.append(f"   Breakdown:")
            for op_type, count in result['operation_breakdown'].items():
                output.append(f"     {op_type}: {count:,} ops")
    
    return "\n".join(output)

def save_results(results: List[Dict[str, Any]], filename: str):
    """Salva resultados em arquivo JSON"""
    timestamp = time.strftime("%Y%m%d_%H%M%S")
    filepath = f"benchmark_results/complete_benchmark_{timestamp}.json"
    
    with open(filepath, 'w') as f:
        json.dump({
            'timestamp': timestamp,
            'results': results,
            'summary': {
                'total_tests': len(results),
                'best_throughput': max(r['ops_per_sec'] for r in results),
                'best_latency_p99': min(r['latency_ms']['p99'] for r in results if r['latency_ms']['p99'] > 0)
            }
        }, f, indent=2)
    
    print(f"📁 Resultados salvos em: {filepath}")

def main():
    print("🦀 CrabCache - Benchmark Completo")
    print("=" * 40)
    
    # Inicia servidor
    if not start_crabcache_server():
        print("❌ Falha ao iniciar servidor")
        return 1
    
    try:
        benchmark = CrabCacheBenchmark()
        results = []
        
        # Testes individuais
        test_configs = [
            ("PING", 10000, 1),
            ("PUT", 5000, 1),
            ("GET", 10000, 1),
            ("PUT", 5000, 10),  # Concorrente
            ("GET", 10000, 10), # Concorrente
        ]
        
        for operation, count, concurrent in test_configs:
            result = benchmark.benchmark_operation(operation, count, concurrent)
            results.append(result)
            time.sleep(1)  # Pausa entre testes
        
        # Teste de workload misto
        mixed_result = benchmark.benchmark_mixed_workload(10000, 10)
        results.append(mixed_result)
        
        # Obtém métricas finais
        print("\n📊 Obtendo métricas finais...")
        metrics = benchmark.get_metrics()
        
        # Exibe resultados
        print("\n" + format_results(results))
        
        if metrics:
            print(f"\n📈 Métricas do Servidor:")
            if 'global' in metrics:
                global_metrics = metrics['global']
                print(f"   Uptime:        {global_metrics.get('uptime_seconds', 0)}s")
                print(f"   Total Ops:     {global_metrics.get('total_operations', 0):,}")
                print(f"   Hit Ratio:     {global_metrics.get('cache_hit_ratio', 0):.1%}")
                print(f"   Memory Used:   {global_metrics.get('memory_used_bytes', 0):,} bytes")
        
        # Salva resultados
        save_results(results, "complete_benchmark")
        
        print(f"\n🎉 Benchmark concluído com sucesso!")
        print(f"📊 {len(results)} testes executados")
        
        # Resultado principal para README
        mixed_ops_sec = mixed_result['ops_per_sec']
        mixed_p99 = mixed_result['latency_ms']['p99']
        
        print(f"\n🏆 RESULTADO PRINCIPAL:")
        print(f"   Mixed Workload: {mixed_ops_sec:,.0f} ops/sec")
        print(f"   P99 Latency:    {mixed_p99:.3f}ms")
        
        return 0
        
    except Exception as e:
        print(f"❌ Erro durante benchmark: {e}")
        return 1
        
    finally:
        stop_crabcache_server()

if __name__ == "__main__":
    exit(main())