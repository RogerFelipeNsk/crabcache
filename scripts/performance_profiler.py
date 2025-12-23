#!/usr/bin/env python3
"""
Profiler de Performance para CrabCache
Identifica gargalos específicos no sistema
"""

import socket
import time
import threading
import statistics
import json
from concurrent.futures import ThreadPoolExecutor
from dataclasses import dataclass
from typing import List, Dict, Any

@dataclass
class DetailedResult:
    operation: str
    connect_time: float
    send_time: float
    recv_time: float
    parse_time: float
    total_time: float
    response_size: int
    success: bool

class CrabCacheProfiler:
    def __init__(self, host='localhost', port=7001):
        self.host = host
        self.port = port
    
    def profile_single_operation(self, command: str, use_persistent_conn=False, sock=None) -> DetailedResult:
        """Profila uma operação específica com timing detalhado"""
        
        # Timing de conexão
        connect_start = time.perf_counter()
        if sock is None:
            sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
            sock.settimeout(10)
            sock.connect((self.host, self.port))
            should_close = True
        else:
            should_close = False
        connect_time = time.perf_counter() - connect_start
        
        try:
            # Timing de envio
            send_start = time.perf_counter()
            if not command.endswith('\n'):
                command += '\n'
            sock.send(command.encode())
            send_time = time.perf_counter() - send_start
            
            # Timing de recebimento
            recv_start = time.perf_counter()
            response = sock.recv(4096)
            recv_time = time.perf_counter() - recv_start
            
            # Timing de parsing
            parse_start = time.perf_counter()
            response_str = response.decode().strip()
            parse_time = time.perf_counter() - parse_start
            
            total_time = connect_time + send_time + recv_time + parse_time
            
            success = self._is_success(command, response_str)
            
            return DetailedResult(
                operation=command.split()[0],
                connect_time=connect_time,
                send_time=send_time,
                recv_time=recv_time,
                parse_time=parse_time,
                total_time=total_time,
                response_size=len(response),
                success=success
            )
            
        finally:
            if should_close:
                sock.close()
    
    def _is_success(self, command: str, response: str) -> bool:
        """Determina se a resposta indica sucesso"""
        cmd = command.split()[0].upper()
        if cmd == 'PING':
            return response == 'PONG'
        elif cmd == 'PUT':
            return response == 'OK'
        elif cmd == 'GET':
            return not response.startswith('ERROR')
        elif cmd == 'DEL':
            return response == 'OK' or response == 'NULL'
        elif cmd == 'STATS':
            return response.startswith('STATS:')
        return not response.startswith('ERROR')
    
    def profile_connection_overhead(self, num_operations=100) -> Dict[str, Any]:
        """Compara overhead de conexões novas vs persistentes"""
        print("🔍 Analisando overhead de conexões...")
        
        # Teste com conexões novas
        new_conn_results = []
        start_time = time.perf_counter()
        
        for i in range(num_operations):
            result = self.profile_single_operation(f"PUT test_new_{i} value_{i}")
            new_conn_results.append(result)
        
        new_conn_total_time = time.perf_counter() - start_time
        
        # Teste com conexão persistente
        persistent_results = []
        start_time = time.perf_counter()
        
        sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        sock.settimeout(10)
        sock.connect((self.host, self.port))
        
        for i in range(num_operations):
            result = self.profile_single_operation(f"PUT test_persistent_{i} value_{i}", 
                                                 use_persistent_conn=True, sock=sock)
            persistent_results.append(result)
        
        sock.close()
        persistent_total_time = time.perf_counter() - start_time
        
        # Análise
        new_conn_avg = statistics.mean([r.total_time for r in new_conn_results])
        persistent_avg = statistics.mean([r.total_time for r in persistent_results])
        
        new_conn_connect_avg = statistics.mean([r.connect_time for r in new_conn_results])
        persistent_connect_avg = statistics.mean([r.connect_time for r in persistent_results])
        
        return {
            'new_connections': {
                'total_time': new_conn_total_time,
                'avg_per_op': new_conn_avg,
                'avg_connect_time': new_conn_connect_avg,
                'throughput': num_operations / new_conn_total_time
            },
            'persistent_connection': {
                'total_time': persistent_total_time,
                'avg_per_op': persistent_avg,
                'avg_connect_time': persistent_connect_avg,
                'throughput': num_operations / persistent_total_time
            },
            'improvement': {
                'speedup': new_conn_avg / persistent_avg,
                'connect_overhead_reduction': new_conn_connect_avg / persistent_connect_avg if persistent_connect_avg > 0 else float('inf')
            }
        }
    
    def profile_operation_breakdown(self, operations=None, samples_per_op=50) -> Dict[str, Any]:
        """Analisa breakdown de tempo por tipo de operação"""
        if operations is None:
            operations = [
                "PING",
                "PUT test_key test_value",
                "GET test_key",
                "DEL test_key",
                "STATS"
            ]
        
        print("🔍 Analisando breakdown por operação...")
        
        results = {}
        sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        sock.settimeout(10)
        sock.connect((self.host, self.port))
        
        try:
            for operation in operations:
                op_name = operation.split()[0]
                op_results = []
                
                for i in range(samples_per_op):
                    # Personalizar comando se necessário
                    if "test_key" in operation:
                        cmd = operation.replace("test_key", f"profile_key_{i}")
                        cmd = cmd.replace("test_value", f"profile_value_{i}")
                    else:
                        cmd = operation
                    
                    result = self.profile_single_operation(cmd, use_persistent_conn=True, sock=sock)
                    op_results.append(result)
                
                # Calcular estatísticas
                connect_times = [r.connect_time for r in op_results]
                send_times = [r.send_time for r in op_results]
                recv_times = [r.recv_time for r in op_results]
                parse_times = [r.parse_time for r in op_results]
                total_times = [r.total_time for r in op_results]
                response_sizes = [r.response_size for r in op_results]
                
                results[op_name] = {
                    'samples': samples_per_op,
                    'avg_connect_time_ms': statistics.mean(connect_times) * 1000,
                    'avg_send_time_ms': statistics.mean(send_times) * 1000,
                    'avg_recv_time_ms': statistics.mean(recv_times) * 1000,
                    'avg_parse_time_ms': statistics.mean(parse_times) * 1000,
                    'avg_total_time_ms': statistics.mean(total_times) * 1000,
                    'avg_response_size': statistics.mean(response_sizes),
                    'p95_total_time_ms': sorted(total_times)[int(len(total_times) * 0.95)] * 1000,
                    'success_rate': sum(1 for r in op_results if r.success) / len(op_results) * 100
                }
        
        finally:
            sock.close()
        
        return results
    
    def profile_payload_size_impact(self, sizes=[10, 50, 100, 500, 1000, 5000]) -> Dict[str, Any]:
        """Analisa impacto do tamanho do payload na performance"""
        print("🔍 Analisando impacto do tamanho do payload...")
        
        results = {}
        sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        sock.settimeout(10)
        sock.connect((self.host, self.port))
        
        try:
            for size in sizes:
                value = 'x' * size
                size_results = []
                
                for i in range(20):  # 20 amostras por tamanho
                    key = f"payload_test_{size}_{i}"
                    result = self.profile_single_operation(f"PUT {key} {value}", 
                                                         use_persistent_conn=True, sock=sock)
                    size_results.append(result)
                
                avg_time = statistics.mean([r.total_time for r in size_results])
                avg_send_time = statistics.mean([r.send_time for r in size_results])
                avg_recv_time = statistics.mean([r.recv_time for r in size_results])
                
                results[f"{size}B"] = {
                    'payload_size': size,
                    'avg_total_time_ms': avg_time * 1000,
                    'avg_send_time_ms': avg_send_time * 1000,
                    'avg_recv_time_ms': avg_recv_time * 1000,
                    'throughput_ops_sec': 1 / avg_time,
                    'throughput_mb_sec': (size / (1024 * 1024)) / avg_time
                }
        
        finally:
            sock.close()
        
        return results
    
    def profile_concurrency_scaling(self, max_workers=20) -> Dict[str, Any]:
        """Analisa como a performance escala com concorrência"""
        print("🔍 Analisando escalabilidade de concorrência...")
        
        results = {}
        
        for workers in [1, 2, 5, 10, 15, 20]:
            if workers > max_workers:
                break
                
            print(f"   Testando com {workers} workers...")
            
            def worker_task():
                sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
                sock.settimeout(10)
                sock.connect((self.host, self.port))
                
                worker_results = []
                for i in range(50):  # 50 ops por worker
                    result = self.profile_single_operation(f"PUT conc_{workers}_{i} value_{i}", 
                                                         use_persistent_conn=True, sock=sock)
                    worker_results.append(result)
                
                sock.close()
                return worker_results
            
            start_time = time.perf_counter()
            
            with ThreadPoolExecutor(max_workers=workers) as executor:
                futures = [executor.submit(worker_task) for _ in range(workers)]
                all_results = []
                for future in futures:
                    all_results.extend(future.result())
            
            total_time = time.perf_counter() - start_time
            
            avg_latency = statistics.mean([r.total_time for r in all_results])
            total_ops = len(all_results)
            throughput = total_ops / total_time
            
            results[f"{workers}_workers"] = {
                'workers': workers,
                'total_operations': total_ops,
                'total_time': total_time,
                'avg_latency_ms': avg_latency * 1000,
                'throughput_ops_sec': throughput,
                'ops_per_worker': total_ops / workers
            }
        
        return results
    
    def generate_optimization_report(self) -> Dict[str, Any]:
        """Gera relatório completo de otimização"""
        print("🚀 Gerando Relatório de Otimização do CrabCache")
        print("=" * 60)
        
        report = {
            'timestamp': time.time(),
            'connection_analysis': self.profile_connection_overhead(),
            'operation_breakdown': self.profile_operation_breakdown(),
            'payload_analysis': self.profile_payload_size_impact(),
            'concurrency_analysis': self.profile_concurrency_scaling()
        }
        
        # Análise e recomendações
        report['recommendations'] = self.analyze_bottlenecks(report)
        
        return report
    
    def analyze_bottlenecks(self, report: Dict[str, Any]) -> Dict[str, Any]:
        """Analisa gargalos e gera recomendações"""
        recommendations = {
            'critical_issues': [],
            'optimization_opportunities': [],
            'performance_targets': {}
        }
        
        # Análise de conexões
        conn_analysis = report['connection_analysis']
        speedup = conn_analysis['improvement']['speedup']
        if speedup > 2:
            recommendations['critical_issues'].append({
                'issue': 'Connection Overhead',
                'impact': f'{speedup:.1f}x slower with new connections',
                'solution': 'Implement connection pooling in production clients'
            })
        
        # Análise de operações
        op_breakdown = report['operation_breakdown']
        for op, stats in op_breakdown.items():
            total_time = stats['avg_total_time_ms']
            send_time = stats['avg_send_time_ms']
            recv_time = stats['avg_recv_time_ms']
            
            if send_time > total_time * 0.3:
                recommendations['optimization_opportunities'].append({
                    'area': f'{op} Send Performance',
                    'current': f'{send_time:.2f}ms ({send_time/total_time*100:.1f}% of total)',
                    'suggestion': 'Optimize command serialization and TCP send buffer'
                })
            
            if recv_time > total_time * 0.4:
                recommendations['optimization_opportunities'].append({
                    'area': f'{op} Receive Performance',
                    'current': f'{recv_time:.2f}ms ({recv_time/total_time*100:.1f}% of total)',
                    'suggestion': 'Optimize response generation and TCP receive buffer'
                })
        
        # Análise de payload
        payload_analysis = report['payload_analysis']
        small_payload = payload_analysis.get('10B', {})
        large_payload = payload_analysis.get('5000B', {})
        
        if small_payload and large_payload:
            small_time = small_payload['avg_total_time_ms']
            large_time = large_payload['avg_total_time_ms']
            scaling_factor = large_time / small_time
            
            if scaling_factor > 10:  # Não deveria escalar tão mal
                recommendations['critical_issues'].append({
                    'issue': 'Poor Payload Scaling',
                    'impact': f'{scaling_factor:.1f}x slower for large payloads',
                    'solution': 'Implement zero-copy operations and streaming'
                })
        
        # Targets de performance
        current_best_throughput = max([
            stats['throughput_ops_sec'] 
            for stats in report['concurrency_analysis'].values()
        ])
        
        recommendations['performance_targets'] = {
            'current_peak_throughput': f'{current_best_throughput:.0f} ops/sec',
            'short_term_target': f'{current_best_throughput * 2:.0f} ops/sec (2x improvement)',
            'medium_term_target': f'{current_best_throughput * 5:.0f} ops/sec (5x improvement)',
            'long_term_target': '20,000+ ops/sec (Redis competitive)'
        }
        
        return recommendations
    
    def print_report(self, report: Dict[str, Any]):
        """Imprime relatório formatado"""
        print("\n" + "=" * 80)
        print("📊 RELATÓRIO DE ANÁLISE DE PERFORMANCE - CrabCache")
        print("=" * 80)
        
        # Conexões
        print("\n🔗 ANÁLISE DE CONEXÕES:")
        conn = report['connection_analysis']
        print(f"   Nova conexão por operação: {conn['new_connections']['avg_per_op']*1000:.2f}ms")
        print(f"   Conexão persistente:       {conn['persistent_connection']['avg_per_op']*1000:.2f}ms")
        print(f"   Melhoria com persistência: {conn['improvement']['speedup']:.1f}x mais rápido")
        
        # Breakdown por operação
        print("\n⚡ BREAKDOWN POR OPERAÇÃO:")
        for op, stats in report['operation_breakdown'].items():
            print(f"   🔸 {op}:")
            print(f"      Total: {stats['avg_total_time_ms']:.2f}ms")
            print(f"      Send:  {stats['avg_send_time_ms']:.2f}ms ({stats['avg_send_time_ms']/stats['avg_total_time_ms']*100:.1f}%)")
            print(f"      Recv:  {stats['avg_recv_time_ms']:.2f}ms ({stats['avg_recv_time_ms']/stats['avg_total_time_ms']*100:.1f}%)")
            print(f"      Parse: {stats['avg_parse_time_ms']:.2f}ms ({stats['avg_parse_time_ms']/stats['avg_total_time_ms']*100:.1f}%)")
        
        # Payload
        print("\n📦 ANÁLISE DE PAYLOAD:")
        for size, stats in report['payload_analysis'].items():
            print(f"   {size:>6}: {stats['avg_total_time_ms']:.2f}ms ({stats['throughput_ops_sec']:.0f} ops/sec)")
        
        # Concorrência
        print("\n👥 ANÁLISE DE CONCORRÊNCIA:")
        for workers, stats in report['concurrency_analysis'].items():
            print(f"   {stats['workers']:2d} workers: {stats['throughput_ops_sec']:.0f} ops/sec (latência: {stats['avg_latency_ms']:.2f}ms)")
        
        # Recomendações
        print("\n🎯 RECOMENDAÇÕES:")
        recs = report['recommendations']
        
        if recs['critical_issues']:
            print("   🔴 PROBLEMAS CRÍTICOS:")
            for issue in recs['critical_issues']:
                print(f"      • {issue['issue']}: {issue['impact']}")
                print(f"        Solução: {issue['solution']}")
        
        if recs['optimization_opportunities']:
            print("   🟡 OPORTUNIDADES DE OTIMIZAÇÃO:")
            for opp in recs['optimization_opportunities']:
                print(f"      • {opp['area']}: {opp['current']}")
                print(f"        Sugestão: {opp['suggestion']}")
        
        print("   📈 METAS DE PERFORMANCE:")
        targets = recs['performance_targets']
        for target, value in targets.items():
            print(f"      • {target.replace('_', ' ').title()}: {value}")
        
        print("\n" + "=" * 80)

def main():
    import argparse
    
    parser = argparse.ArgumentParser(description='Profiler de Performance do CrabCache')
    parser.add_argument('--host', default='localhost', help='Host do CrabCache')
    parser.add_argument('--port', type=int, default=7001, help='Porta do CrabCache')
    parser.add_argument('--output', help='Arquivo para salvar relatório JSON')
    
    args = parser.parse_args()
    
    profiler = CrabCacheProfiler(args.host, args.port)
    report = profiler.generate_optimization_report()
    
    profiler.print_report(report)
    
    if args.output:
        with open(args.output, 'w') as f:
            json.dump(report, f, indent=2, default=str)
        print(f"\n💾 Relatório salvo em: {args.output}")

if __name__ == '__main__':
    main()