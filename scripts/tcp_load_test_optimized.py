#!/usr/bin/env python3
"""
Teste de carga TCP otimizado para CrabCache
Versão com connection pooling e melhor gerenciamento de recursos
"""

import socket
import time
import random
import string
import threading
import json
import statistics
from concurrent.futures import ThreadPoolExecutor, as_completed
from dataclasses import dataclass
from typing import List, Dict, Any, Optional

@dataclass
class TestResult:
    operation: str
    success: bool
    duration: float
    response: str = ""
    error: str = ""

class OptimizedCrabCacheLoadTester:
    def __init__(self, host: str = 'localhost', port: int = 7001):
        self.host = host
        self.port = port
        self.lock = threading.Lock()
        self.results = []
    
    def create_connection(self) -> socket.socket:
        """Cria conexão TCP com o CrabCache"""
        sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        sock.settimeout(10)  # Timeout maior
        sock.connect((self.host, self.port))
        return sock
    
    def send_command_with_connection(self, sock: socket.socket, command: str) -> TestResult:
        """Envia comando usando conexão existente"""
        start = time.time()
        try:
            if not command.endswith('\n'):
                command += '\n'
            
            sock.send(command.encode())
            response = sock.recv(4096).decode().strip()
            
            duration = time.time() - start
            success = self._is_success(command, response)
            
            return TestResult(
                operation=command.split()[0],
                success=success,
                duration=duration,
                response=response
            )
            
        except Exception as e:
            duration = time.time() - start
            return TestResult(
                operation=command.split()[0] if command else "UNKNOWN",
                success=False,
                duration=duration,
                error=str(e)
            )
    
    def _is_success(self, command: str, response: str) -> bool:
        """Determina se a resposta indica sucesso"""
        cmd = command.split()[0].upper()
        
        if cmd == 'PING':
            return response == 'PONG'
        elif cmd == 'PUT':
            return response == 'OK'
        elif cmd == 'DEL':
            return response == 'OK' or response == 'NULL'  # NULL = chave não existia
        elif cmd == 'GET':
            return not response.startswith('ERROR')  # NULL é válido
        elif cmd == 'STATS':
            return response.startswith('STATS:')
        else:
            return not response.startswith('ERROR')
    
    def random_key(self, prefix: str = "load") -> str:
        """Gera chave aleatória"""
        suffix = ''.join(random.choices(string.ascii_lowercase + string.digits, k=8))
        return f"{prefix}:{suffix}"
    
    def random_value(self, size: int = 50) -> str:
        """Gera valor aleatório"""
        return ''.join(random.choices(string.ascii_letters + string.digits, k=size))
    
    def worker_with_persistent_connection(self, worker_id: int, duration: int, ops_per_second: int = 100) -> List[TestResult]:
        """Worker que mantém conexão persistente"""
        results = []
        end_time = time.time() + duration
        keys_created = []
        
        # Calcular intervalo entre operações
        interval = 1.0 / ops_per_second if ops_per_second > 0 else 0
        
        try:
            # Criar conexão persistente para este worker
            sock = self.create_connection()
            
            # Testar conectividade
            test_result = self.send_command_with_connection(sock, "PING")
            if not test_result.success:
                sock.close()
                return [TestResult("CONNECTION", False, 0, error="Failed to connect")]
            
            operation_count = 0
            while time.time() < end_time:
                operation_start = time.time()
                
                # Escolher operação com pesos realistas
                operation = random.choices(
                    ['PUT', 'GET', 'DEL', 'PING', 'STATS'],
                    weights=[30, 50, 10, 5, 5],
                    k=1
                )[0]
                
                if operation == 'PUT':
                    key = f"worker_{worker_id}_{self.random_key()}"
                    value = self.random_value(random.randint(10, 100))
                    command = f"PUT {key} {value}"
                    
                    result = self.send_command_with_connection(sock, command)
                    if result.success:
                        keys_created.append(key)
                    results.append(result)
                    
                elif operation == 'GET':
                    if keys_created and random.random() < 0.9:  # 90% chance de usar chave existente
                        key = random.choice(keys_created)
                    else:
                        key = f"worker_{worker_id}_{self.random_key()}"
                    
                    result = self.send_command_with_connection(sock, f"GET {key}")
                    results.append(result)
                    
                elif operation == 'DEL':
                    if keys_created and random.random() < 0.8:  # 80% chance de deletar chave existente
                        key = keys_created.pop(random.randint(0, len(keys_created) - 1))
                    else:
                        key = f"worker_{worker_id}_{self.random_key()}"
                    
                    result = self.send_command_with_connection(sock, f"DEL {key}")
                    results.append(result)
                    
                elif operation == 'PING':
                    result = self.send_command_with_connection(sock, "PING")
                    results.append(result)
                    
                elif operation == 'STATS':
                    result = self.send_command_with_connection(sock, "STATS")
                    results.append(result)
                
                operation_count += 1
                
                # Controlar taxa de operações
                if interval > 0:
                    elapsed = time.time() - operation_start
                    sleep_time = max(0, interval - elapsed)
                    if sleep_time > 0:
                        time.sleep(sleep_time)
            
            sock.close()
            print(f"📊 Worker {worker_id} concluído: {len(results)} ops ({operation_count} tentativas)")
            
        except Exception as e:
            print(f"❌ Worker {worker_id} falhou: {e}")
            results.append(TestResult("WORKER_ERROR", False, 0, error=str(e)))
        
        return results
    
    def run_optimized_load_test(self, concurrent_users: int, duration: int, ops_per_second: int = 100) -> Dict[str, Any]:
        """Executa teste de carga otimizado"""
        print(f"🚀 Teste de Carga TCP Otimizado - CrabCache")
        print(f"=" * 50)
        print(f"Host: {self.host}:{self.port}")
        print(f"Usuários concorrentes: {concurrent_users}")
        print(f"Duração: {duration}s")
        print(f"Taxa alvo: {ops_per_second} ops/sec por usuário")
        print(f"Taxa total esperada: {concurrent_users * ops_per_second} ops/sec")
        print()
        
        # Testar conectividade
        try:
            sock = self.create_connection()
            sock.send(b"PING\n")
            response = sock.recv(1024).decode().strip()
            sock.close()
            if response != "PONG":
                return {"error": f"Conectividade falhou: {response}"}
            print("✅ Conectividade TCP verificada")
        except Exception as e:
            return {"error": f"Falha na conectividade: {e}"}
        
        print("🏁 Iniciando teste de carga otimizado...")
        start_time = time.time()
        
        # Executar workers com conexões persistentes
        with ThreadPoolExecutor(max_workers=concurrent_users) as executor:
            futures = [
                executor.submit(self.worker_with_persistent_connection, i, duration, ops_per_second)
                for i in range(concurrent_users)
            ]
            
            all_results = []
            for future in as_completed(futures):
                try:
                    worker_results = future.result()
                    all_results.extend(worker_results)
                except Exception as e:
                    print(f"❌ Erro em worker: {e}")
        
        total_time = time.time() - start_time
        print(f"⏱️  Teste concluído em {total_time:.2f}s")
        
        return self.analyze_results(all_results, total_time, concurrent_users)
    
    def analyze_results(self, results: List[TestResult], total_time: float, concurrent_users: int) -> Dict[str, Any]:
        """Analisa resultados do teste"""
        if not results:
            return {"error": "Nenhum resultado coletado"}
        
        # Estatísticas por operação
        ops_stats = {}
        all_durations = []
        
        for result in results:
            op = result.operation
            if op not in ops_stats:
                ops_stats[op] = {
                    'total': 0,
                    'success': 0,
                    'errors': 0,
                    'durations': []
                }
            
            ops_stats[op]['total'] += 1
            if result.success:
                ops_stats[op]['success'] += 1
            else:
                ops_stats[op]['errors'] += 1
            
            ops_stats[op]['durations'].append(result.duration)
            all_durations.append(result.duration)
        
        # Calcular estatísticas globais
        all_durations.sort()
        total_ops = len(results)
        successful_ops = sum(1 for r in results if r.success)
        
        summary = {
            'total_operations': total_ops,
            'successful_operations': successful_ops,
            'success_rate': (successful_ops / total_ops * 100) if total_ops > 0 else 0,
            'actual_ops_per_second': total_ops / total_time if total_time > 0 else 0,
            'avg_latency_ms': statistics.mean(all_durations) * 1000,
            'p50_latency_ms': all_durations[int(len(all_durations) * 0.5)] * 1000,
            'p95_latency_ms': all_durations[int(len(all_durations) * 0.95)] * 1000,
            'p99_latency_ms': all_durations[int(len(all_durations) * 0.99)] * 1000,
            'concurrent_users': concurrent_users,
            'test_duration': total_time
        }
        
        # Estatísticas por operação
        operations = {}
        for op, stats in ops_stats.items():
            if stats['durations']:
                stats['durations'].sort()
                operations[op] = {
                    'total': stats['total'],
                    'success': stats['success'],
                    'success_rate': (stats['success'] / stats['total'] * 100) if stats['total'] > 0 else 0,
                    'ops_per_second': stats['total'] / total_time if total_time > 0 else 0,
                    'avg_latency_ms': statistics.mean(stats['durations']) * 1000,
                    'p95_latency_ms': stats['durations'][int(len(stats['durations']) * 0.95)] * 1000
                }
        
        # Imprimir resultados
        self.print_results(summary, operations)
        
        return {
            'global_metrics': summary,
            'operations': operations,
            'raw_results': results
        }
    
    def print_results(self, summary: Dict[str, Any], operations: Dict[str, Any]):
        """Imprime resultados formatados"""
        print("\n" + "=" * 60)
        print("📊 RESULTADOS DO TESTE DE CARGA TCP OTIMIZADO")
        print("=" * 60)
        
        print("📈 Métricas Globais:")
        print(f"   Total de operações: {summary['total_operations']:,}")
        print(f"   Operações bem-sucedidas: {summary['successful_operations']:,}")
        print(f"   Taxa de sucesso: {summary['success_rate']:.1f}%")
        print(f"   Throughput: {summary['actual_ops_per_second']:.1f} ops/sec")
        print(f"   Latência média: {summary['avg_latency_ms']:.2f}ms")
        print(f"   P50: {summary['p50_latency_ms']:.2f}ms")
        print(f"   P95: {summary['p95_latency_ms']:.2f}ms")
        print(f"   P99: {summary['p99_latency_ms']:.2f}ms")
        
        print("\n📋 Métricas por Operação:")
        for op, stats in sorted(operations.items()):
            print(f"   🔸 {op}:")
            print(f"      Total: {stats['total']:,} | Sucesso: {stats['success_rate']:.1f}%")
            print(f"      Throughput: {stats['ops_per_second']:.1f} ops/sec")
            print(f"      Latência: avg={stats['avg_latency_ms']:.2f}ms, p95={stats['p95_latency_ms']:.2f}ms")
        
        print("\n" + "=" * 60)

def main():
    import argparse
    
    parser = argparse.ArgumentParser(description='Teste de carga TCP otimizado para CrabCache')
    parser.add_argument('--host', default='localhost', help='Host do CrabCache')
    parser.add_argument('--port', type=int, default=7001, help='Porta do CrabCache')
    parser.add_argument('--users', type=int, default=10, help='Número de usuários concorrentes')
    parser.add_argument('--duration', type=int, default=30, help='Duração do teste em segundos')
    parser.add_argument('--ops-per-sec', type=int, default=100, help='Operações por segundo por usuário')
    parser.add_argument('--output', help='Arquivo para salvar resultados JSON')
    
    args = parser.parse_args()
    
    tester = OptimizedCrabCacheLoadTester(args.host, args.port)
    results = tester.run_optimized_load_test(args.users, args.duration, args.ops_per_sec)
    
    if 'error' in results:
        print(f"❌ Erro: {results['error']}")
        return 1
    
    # Salvar resultados se solicitado
    if args.output:
        with open(args.output, 'w') as f:
            json.dump(results, f, indent=2, default=str)
        print(f"💾 Resultados salvos em: {args.output}")
    
    # Código de saída baseado na taxa de sucesso
    success_rate = results['global_metrics']['success_rate']
    if success_rate < 95:
        print(f"⚠️  Taxa de sucesso baixa: {success_rate:.1f}%")
        return 1
    
    return 0

if __name__ == '__main__':
    exit(main())