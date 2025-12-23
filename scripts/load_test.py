#!/usr/bin/env python3
"""
Teste de Carga para CrabCache via HTTP Wrapper
"""

import requests
import time
import threading
import random
import string
import json
import os
from concurrent.futures import ThreadPoolExecutor, as_completed
from dataclasses import dataclass
from typing import List, Dict, Any

@dataclass
class TestResult:
    operation: str
    success: bool
    duration: float
    error: str = None

class CrabCacheLoadTester:
    def __init__(self, base_url: str):
        self.base_url = base_url.rstrip('/')
        self.session = requests.Session()
        self.results: List[TestResult] = []
        self.lock = threading.Lock()
        
    def random_key(self, prefix: str = "test") -> str:
        """Gera chave aleatória"""
        suffix = ''.join(random.choices(string.ascii_lowercase + string.digits, k=8))
        return f"{prefix}:{suffix}"
    
    def random_value(self, size: int = 50) -> str:
        """Gera valor aleatório"""
        return ''.join(random.choices(string.ascii_letters + string.digits + ' ', k=size))
    
    def add_result(self, result: TestResult):
        """Thread-safe adicionar resultado"""
        with self.lock:
            self.results.append(result)
    
    def test_ping(self) -> TestResult:
        """Teste PING"""
        start = time.time()
        try:
            response = self.session.get(f"{self.base_url}/ping", timeout=5)
            duration = time.time() - start
            
            if response.status_code == 200:
                data = response.json()
                success = data.get('success', False)
                return TestResult("PING", success, duration)
            else:
                return TestResult("PING", False, duration, f"HTTP {response.status_code}")
                
        except Exception as e:
            duration = time.time() - start
            return TestResult("PING", False, duration, str(e))
    
    def test_put(self, key: str = None, value: str = None, ttl: int = None) -> TestResult:
        """Teste PUT"""
        start = time.time()
        try:
            key = key or self.random_key()
            value = value or self.random_value()
            
            payload = {"key": key, "value": value}
            if ttl:
                payload["ttl"] = ttl
            
            response = self.session.post(
                f"{self.base_url}/put",
                json=payload,
                timeout=5
            )
            duration = time.time() - start
            
            if response.status_code == 200:
                data = response.json()
                success = data.get('success', False)
                return TestResult("PUT", success, duration)
            else:
                return TestResult("PUT", False, duration, f"HTTP {response.status_code}")
                
        except Exception as e:
            duration = time.time() - start
            return TestResult("PUT", False, duration, str(e))
    
    def test_get(self, key: str) -> TestResult:
        """Teste GET"""
        start = time.time()
        try:
            response = self.session.get(f"{self.base_url}/get/{key}", timeout=5)
            duration = time.time() - start
            
            if response.status_code == 200:
                data = response.json()
                success = data.get('success', False)
                return TestResult("GET", success, duration)
            else:
                return TestResult("GET", False, duration, f"HTTP {response.status_code}")
                
        except Exception as e:
            duration = time.time() - start
            return TestResult("GET", False, duration, str(e))
    
    def test_delete(self, key: str) -> TestResult:
        """Teste DELETE"""
        start = time.time()
        try:
            response = self.session.delete(f"{self.base_url}/delete/{key}", timeout=5)
            duration = time.time() - start
            
            if response.status_code == 200:
                data = response.json()
                success = data.get('success', False)
                return TestResult("DELETE", success, duration)
            else:
                return TestResult("DELETE", False, duration, f"HTTP {response.status_code}")
                
        except Exception as e:
            duration = time.time() - start
            return TestResult("DELETE", False, duration, str(e))
    
    def test_stats(self) -> TestResult:
        """Teste STATS"""
        start = time.time()
        try:
            response = self.session.get(f"{self.base_url}/stats", timeout=5)
            duration = time.time() - start
            
            if response.status_code == 200:
                data = response.json()
                success = data.get('success', False)
                return TestResult("STATS", success, duration)
            else:
                return TestResult("STATS", False, duration, f"HTTP {response.status_code}")
                
        except Exception as e:
            duration = time.time() - start
            return TestResult("STATS", False, duration, str(e))
    
    def worker_mixed_operations(self, duration: int) -> List[TestResult]:
        """Worker que executa operações mistas"""
        results = []
        end_time = time.time() + duration
        keys_created = []
        
        while time.time() < end_time:
            # Escolher operação aleatória
            operation = random.choices(
                ['PUT', 'GET', 'DELETE', 'PING', 'STATS'],
                weights=[40, 30, 10, 10, 10],  # PUT e GET mais frequentes
                k=1
            )[0]
            
            if operation == 'PUT':
                key = self.random_key()
                result = self.test_put(key, ttl=random.randint(60, 3600))
                if result.success:
                    keys_created.append(key)
                results.append(result)
                
            elif operation == 'GET':
                if keys_created and random.random() < 0.7:  # 70% chance de usar chave existente
                    key = random.choice(keys_created)
                else:
                    key = self.random_key()  # Chave que não existe
                results.append(self.test_get(key))
                
            elif operation == 'DELETE':
                if keys_created:
                    key = keys_created.pop(random.randint(0, len(keys_created) - 1))
                    results.append(self.test_delete(key))
                else:
                    results.append(self.test_delete(self.random_key()))
                    
            elif operation == 'PING':
                results.append(self.test_ping())
                
            elif operation == 'STATS':
                results.append(self.test_stats())
            
            # Pequena pausa para não sobrecarregar
            time.sleep(random.uniform(0.01, 0.1))
        
        return results
    
    def run_load_test(self, concurrent_users: int, duration: int) -> Dict[str, Any]:
        """Executa teste de carga"""
        print(f"🚀 Iniciando teste de carga:")
        print(f"   Usuários concorrentes: {concurrent_users}")
        print(f"   Duração: {duration}s")
        print(f"   URL: {self.base_url}")
        print()
        
        # Verificar se o serviço está disponível
        try:
            health_check = self.session.get(f"{self.base_url}/health", timeout=10)
            if health_check.status_code != 200:
                raise Exception(f"Health check falhou: {health_check.status_code}")
            print("✅ Serviço disponível")
        except Exception as e:
            print(f"❌ Serviço indisponível: {e}")
            return {"error": str(e)}
        
        # Executar teste
        start_time = time.time()
        
        with ThreadPoolExecutor(max_workers=concurrent_users) as executor:
            futures = [
                executor.submit(self.worker_mixed_operations, duration)
                for _ in range(concurrent_users)
            ]
            
            # Coletar resultados
            all_results = []
            for future in as_completed(futures):
                try:
                    worker_results = future.result()
                    all_results.extend(worker_results)
                except Exception as e:
                    print(f"❌ Erro em worker: {e}")
        
        total_time = time.time() - start_time
        
        # Analisar resultados
        return self.analyze_results(all_results, total_time)
    
    def analyze_results(self, results: List[TestResult], total_time: float) -> Dict[str, Any]:
        """Analisa resultados do teste"""
        if not results:
            return {"error": "Nenhum resultado coletado"}
        
        # Estatísticas por operação
        ops_stats = {}
        for result in results:
            op = result.operation
            if op not in ops_stats:
                ops_stats[op] = {
                    'total': 0,
                    'success': 0,
                    'durations': []
                }
            
            ops_stats[op]['total'] += 1
            if result.success:
                ops_stats[op]['success'] += 1
            ops_stats[op]['durations'].append(result.duration)
        
        # Calcular métricas
        summary = {
            'total_operations': len(results),
            'total_time': total_time,
            'ops_per_second': len(results) / total_time,
            'operations': {}
        }
        
        for op, stats in ops_stats.items():
            durations = stats['durations']
            durations.sort()
            
            summary['operations'][op] = {
                'total': stats['total'],
                'success': stats['success'],
                'success_rate': stats['success'] / stats['total'] * 100,
                'avg_duration': sum(durations) / len(durations),
                'p50_duration': durations[len(durations) // 2],
                'p95_duration': durations[int(len(durations) * 0.95)],
                'p99_duration': durations[int(len(durations) * 0.99)],
                'min_duration': min(durations),
                'max_duration': max(durations)
            }
        
        return summary

def main():
    # Configuração via variáveis de ambiente
    base_url = os.environ.get('CRABCACHE_HTTP_URL', 'http://localhost:8000')
    duration = int(os.environ.get('TEST_DURATION', '60'))
    concurrent_users = int(os.environ.get('CONCURRENT_USERS', '10'))
    
    # Executar teste
    tester = CrabCacheLoadTester(base_url)
    results = tester.run_load_test(concurrent_users, duration)
    
    # Exibir resultados
    if 'error' in results:
        print(f"❌ Erro: {results['error']}")
        return 1
    
    print("📊 Resultados do Teste de Carga")
    print("=" * 50)
    print(f"Total de operações: {results['total_operations']}")
    print(f"Tempo total: {results['total_time']:.2f}s")
    print(f"Operações/segundo: {results['ops_per_second']:.2f}")
    print()
    
    for op, stats in results['operations'].items():
        print(f"📈 {op}:")
        print(f"   Total: {stats['total']}")
        print(f"   Sucesso: {stats['success']} ({stats['success_rate']:.1f}%)")
        print(f"   Latência média: {stats['avg_duration']*1000:.2f}ms")
        print(f"   P50: {stats['p50_duration']*1000:.2f}ms")
        print(f"   P95: {stats['p95_duration']*1000:.2f}ms")
        print(f"   P99: {stats['p99_duration']*1000:.2f}ms")
        print()
    
    # Salvar resultados em JSON
    with open('/tmp/load_test_results.json', 'w') as f:
        json.dump(results, f, indent=2)
    
    print("💾 Resultados salvos em /tmp/load_test_results.json")
    return 0

if __name__ == '__main__':
    exit(main())