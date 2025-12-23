#!/usr/bin/env python3
"""
Teste de Carga TCP Nativo para CrabCache
Conecta diretamente ao protocolo TCP do CrabCache (sem HTTP wrapper)
"""

import socket
import time
import threading
import random
import string
import json
import os
import sys
from concurrent.futures import ThreadPoolExecutor, as_completed
from dataclasses import dataclass
from typing import List, Dict, Any
import argparse
import statistics

@dataclass
class TestResult:
    operation: str
    success: bool
    duration: float
    error: str = None
    response: str = None

class CrabCacheTCPTester:
    def __init__(self, host: str = 'localhost', port: int = 7001):
        self.host = host
        self.port = port
        self.results: List[TestResult] = []
        self.lock = threading.Lock()
        
    def create_connection(self) -> socket.socket:
        """Cria conexão TCP com o CrabCache"""
        sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        sock.settimeout(5)
        sock.connect((self.host, self.port))
        return sock
    
    def send_command(self, command: str) -> TestResult:
        """Envia comando TCP e mede latência"""
        start = time.time()
        try:
            sock = self.create_connection()
            
            if not command.endswith('\r\n'):
                command += '\r\n'
            
            sock.send(command.encode())
            response = sock.recv(4096).decode().strip()
            sock.close()
            
            duration = time.time() - start
            
            # Determinar sucesso baseado na resposta
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
        elif cmd in ['PUT', 'DEL', 'EXPIRE']:
            # DEL retorna OK mesmo se a chave não existir (idempotente)
            return response == 'OK' or (cmd == 'DEL' and response == 'NULL')
        elif cmd == 'GET':
            # GET é sucesso se não for erro - NULL é resposta válida para chave inexistente
            return not response.startswith('ERROR')
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
    
    def add_result(self, result: TestResult):
        """Thread-safe adicionar resultado"""
        with self.lock:
            self.results.append(result)
    
    def worker_mixed_operations(self, duration: int, ops_per_second: int = 100) -> List[TestResult]:
        """Worker que executa operações mistas com taxa controlada"""
        results = []
        end_time = time.time() + duration
        keys_created = []
        
        # Calcular intervalo entre operações
        interval = 1.0 / ops_per_second if ops_per_second > 0 else 0
        
        while time.time() < end_time:
            operation_start = time.time()
            
            # Escolher operação aleatória com pesos realistas
            operation = random.choices(
                ['PUT', 'GET', 'DEL', 'PING', 'STATS'],
                weights=[30, 50, 10, 5, 5],  # GET mais frequente, como uso real
                k=1
            )[0]
            
            if operation == 'PUT':
                key = self.random_key()
                value = self.random_value(random.randint(10, 200))
                ttl = random.choice([None, 300, 600, 1800, 3600])  # TTL variado
                
                if ttl:
                    command = f"PUT {key} {value} {ttl}"
                else:
                    command = f"PUT {key} {value}"
                
                result = self.send_command(command)
                if result.success:
                    keys_created.append(key)
                results.append(result)
                
            elif operation == 'GET':
                if keys_created and random.random() < 0.8:  # 80% chance de usar chave existente
                    key = random.choice(keys_created)
                else:
                    key = self.random_key()  # Chave que pode não existir
                
                result = self.send_command(f"GET {key}")
                results.append(result)
                
            elif operation == 'DEL':
                if keys_created and random.random() < 0.7:  # 70% chance de deletar chave existente
                    key = keys_created.pop(random.randint(0, len(keys_created) - 1))
                else:
                    key = self.random_key()
                
                result = self.send_command(f"DEL {key}")
                results.append(result)
                
            elif operation == 'PING':
                result = self.send_command("PING")
                results.append(result)
                
            elif operation == 'STATS':
                result = self.send_command("STATS")
                results.append(result)
            
            # Controlar taxa de operações
            if interval > 0:
                elapsed = time.time() - operation_start
                sleep_time = max(0, interval - elapsed)
                if sleep_time > 0:
                    time.sleep(sleep_time)
        
        return results
    
    def run_load_test(self, concurrent_users: int, duration: int, ops_per_second: int = 100) -> Dict[str, Any]:
        """Executa teste de carga TCP nativo"""
        print(f"🚀 Teste de Carga TCP Nativo - CrabCache")
        print(f"=" * 50)
        print(f"Host: {self.host}:{self.port}")
        print(f"Usuários concorrentes: {concurrent_users}")
        print(f"Duração: {duration}s")
        print(f"Taxa alvo: {ops_per_second} ops/sec por usuário")
        print(f"Taxa total esperada: {ops_per_second * concurrent_users} ops/sec")
        print()
        
        # Verificar conectividade
        try:
            result = self.send_command("PING")
            if not result.success:
                raise Exception(f"PING falhou: {result.error or result.response}")
            print("✅ Conectividade TCP verificada")
        except Exception as e:
            print(f"❌ Falha na conectividade: {e}")
            return {"error": str(e)}
        
        # Executar teste
        start_time = time.time()
        print(f"🏁 Iniciando teste de carga...")
        
        with ThreadPoolExecutor(max_workers=concurrent_users) as executor:
            futures = [
                executor.submit(self.worker_mixed_operations, duration, ops_per_second)
                for _ in range(concurrent_users)
            ]
            
            # Coletar resultados
            all_results = []
            completed = 0
            for future in as_completed(futures):
                try:
                    worker_results = future.result()
                    all_results.extend(worker_results)
                    completed += 1
                    print(f"📊 Worker {completed}/{concurrent_users} concluído ({len(worker_results)} ops)")
                except Exception as e:
                    print(f"❌ Erro em worker: {e}")
        
        total_time = time.time() - start_time
        print(f"⏱️  Teste concluído em {total_time:.2f}s")
        
        # Analisar resultados
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
        
        # Calcular métricas globais
        all_durations.sort()
        total_ops = len(results)
        successful_ops = sum(1 for r in results if r.success)
        
        summary = {
            'test_config': {
                'concurrent_users': concurrent_users,
                'duration': total_time,
                'target_ops_per_sec': 0  # Será calculado
            },
            'global_metrics': {
                'total_operations': total_ops,
                'successful_operations': successful_ops,
                'failed_operations': total_ops - successful_ops,
                'success_rate': (successful_ops / total_ops * 100) if total_ops > 0 else 0,
                'actual_ops_per_second': total_ops / total_time,
                'avg_latency_ms': statistics.mean(all_durations) * 1000,
                'p50_latency_ms': all_durations[len(all_durations) // 2] * 1000,
                'p95_latency_ms': all_durations[int(len(all_durations) * 0.95)] * 1000,
                'p99_latency_ms': all_durations[int(len(all_durations) * 0.99)] * 1000,
                'min_latency_ms': min(all_durations) * 1000,
                'max_latency_ms': max(all_durations) * 1000
            },
            'operations': {}
        }
        
        # Métricas por operação
        for op, stats in ops_stats.items():
            durations = stats['durations']
            durations.sort()
            
            summary['operations'][op] = {
                'total': stats['total'],
                'success': stats['success'],
                'errors': stats['errors'],
                'success_rate': (stats['success'] / stats['total'] * 100) if stats['total'] > 0 else 0,
                'ops_per_second': stats['total'] / total_time,
                'avg_latency_ms': statistics.mean(durations) * 1000,
                'p50_latency_ms': durations[len(durations) // 2] * 1000,
                'p95_latency_ms': durations[int(len(durations) * 0.95)] * 1000,
                'p99_latency_ms': durations[int(len(durations) * 0.99)] * 1000,
                'min_latency_ms': min(durations) * 1000,
                'max_latency_ms': max(durations) * 1000
            }
        
        return summary

def print_results(results: Dict[str, Any]):
    """Imprime resultados formatados"""
    if 'error' in results:
        print(f"❌ Erro: {results['error']}")
        return
    
    global_metrics = results['global_metrics']
    
    print("\n" + "="*60)
    print("📊 RESULTADOS DO TESTE DE CARGA TCP")
    print("="*60)
    
    print(f"📈 Métricas Globais:")
    print(f"   Total de operações: {global_metrics['total_operations']:,}")
    print(f"   Operações bem-sucedidas: {global_metrics['successful_operations']:,}")
    print(f"   Taxa de sucesso: {global_metrics['success_rate']:.1f}%")
    print(f"   Throughput: {global_metrics['actual_ops_per_second']:.1f} ops/sec")
    print(f"   Latência média: {global_metrics['avg_latency_ms']:.2f}ms")
    print(f"   P50: {global_metrics['p50_latency_ms']:.2f}ms")
    print(f"   P95: {global_metrics['p95_latency_ms']:.2f}ms")
    print(f"   P99: {global_metrics['p99_latency_ms']:.2f}ms")
    print()
    
    print("📋 Métricas por Operação:")
    for op, stats in results['operations'].items():
        print(f"   🔸 {op}:")
        print(f"      Total: {stats['total']:,} | Sucesso: {stats['success_rate']:.1f}%")
        print(f"      Throughput: {stats['ops_per_second']:.1f} ops/sec")
        print(f"      Latência: avg={stats['avg_latency_ms']:.2f}ms, p95={stats['p95_latency_ms']:.2f}ms")
    
    print("\n" + "="*60)

def main():
    parser = argparse.ArgumentParser(description='Teste de Carga TCP para CrabCache')
    parser.add_argument('--host', default='localhost', help='Host do CrabCache')
    parser.add_argument('--port', type=int, default=7001, help='Porta do CrabCache')
    parser.add_argument('--users', type=int, default=10, help='Usuários concorrentes')
    parser.add_argument('--duration', type=int, default=60, help='Duração em segundos')
    parser.add_argument('--ops-per-sec', type=int, default=100, help='Operações por segundo por usuário')
    parser.add_argument('--output', help='Arquivo para salvar resultados JSON')
    
    args = parser.parse_args()
    
    # Executar teste
    tester = CrabCacheTCPTester(args.host, args.port)
    results = tester.run_load_test(args.users, args.duration, args.ops_per_sec)
    
    # Exibir resultados
    print_results(results)
    
    # Salvar resultados se solicitado
    if args.output:
        with open(args.output, 'w') as f:
            json.dump(results, f, indent=2)
        print(f"💾 Resultados salvos em: {args.output}")
    
    # Código de saída baseado na taxa de sucesso
    if 'error' in results:
        return 1
    
    success_rate = results['global_metrics']['success_rate']
    if success_rate < 95:
        print(f"⚠️  Taxa de sucesso baixa: {success_rate:.1f}%")
        return 1
    
    return 0

if __name__ == '__main__':
    exit(main())