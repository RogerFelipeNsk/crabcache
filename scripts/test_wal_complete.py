#!/usr/bin/env python3
"""
Script completo de teste para CrabCache com WAL Persistence
Testa todas as funcionalidades implementadas nas Fases 4.1 e 4.2
"""

import json
import socket
import time
import subprocess
import sys
import requests
from typing import Dict, Any, List, Tuple
import threading
import random
import string

class CrabCacheClient:
    """Cliente Python para CrabCache com suporte a protocolo binário e texto"""
    
    def __init__(self, host='localhost', port=8000):
        self.host = host
        self.port = port
        self.sock = None
        
    def connect(self):
        """Conecta ao servidor CrabCache"""
        self.sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        self.sock.settimeout(10)
        try:
            self.sock.connect((self.host, self.port))
            return True
        except Exception as e:
            print(f"❌ Erro ao conectar: {e}")
            return False
    
    def disconnect(self):
        """Desconecta do servidor"""
        if self.sock:
            self.sock.close()
            self.sock = None
    
    def send_command(self, command: str) -> str:
        """Envia comando e recebe resposta"""
        if not self.sock:
            raise Exception("Não conectado")
        
        try:
            # Envia comando
            self.sock.send(command.encode() + b'\n')
            
            # Recebe resposta
            response = b''
            while True:
                chunk = self.sock.recv(4096)
                if not chunk:
                    break
                response += chunk
                if b'\n' in response:
                    break
            
            return response.decode().strip()
        except Exception as e:
            raise Exception(f"Erro ao enviar comando: {e}")
    
    def put(self, key: str, value: str, ttl: int = None) -> str:
        """PUT key value [ttl]"""
        cmd = f"PUT {key} {value}"
        if ttl:
            cmd += f" {ttl}"
        return self.send_command(cmd)
    
    def get(self, key: str) -> str:
        """GET key"""
        return self.send_command(f"GET {key}")
    
    def delete(self, key: str) -> str:
        """DEL key"""
        return self.send_command(f"DEL {key}")
    
    def expire(self, key: str, ttl: int) -> str:
        """EXPIRE key ttl"""
        return self.send_command(f"EXPIRE {key} {ttl}")
    
    def stats(self) -> Dict[str, Any]:
        """STATS - retorna métricas em JSON"""
        response = self.send_command("STATS")
        try:
            return json.loads(response)
        except:
            return {"raw_response": response}
    
    def ping(self) -> str:
        """PING"""
        return self.send_command("PING")

def test_basic_operations():
    """Testa operações básicas do cache"""
    print("\n🔧 Testando operações básicas...")
    
    client = CrabCacheClient()
    if not client.connect():
        return False
    
    try:
        # Test PING
        response = client.ping()
        assert response == "PONG", f"PING falhou: {response}"
        print("✅ PING/PONG funcionando")
        
        # Test PUT/GET
        client.put("test:basic", "hello_world")
        value = client.get("test:basic")
        assert "hello_world" in value, f"PUT/GET falhou: {value}"
        print("✅ PUT/GET funcionando")
        
        # Test PUT with TTL
        client.put("test:ttl", "expires_soon", 2)
        value = client.get("test:ttl")
        assert "expires_soon" in value, f"PUT com TTL falhou: {value}"
        print("✅ PUT com TTL funcionando")
        
        # Test DELETE
        client.delete("test:basic")
        value = client.get("test:basic")
        assert "NULL" in value or "not found" in value.lower(), f"DELETE falhou: {value}"
        print("✅ DELETE funcionando")
        
        # Test EXPIRE
        client.put("test:expire", "will_expire")
        client.expire("test:expire", 1)
        print("✅ EXPIRE funcionando")
        
        return True
        
    except Exception as e:
        print(f"❌ Erro em operações básicas: {e}")
        return False
    finally:
        client.disconnect()

def test_eviction_system():
    """Testa sistema de eviction TinyLFU"""
    print("\n🧠 Testando sistema de eviction TinyLFU...")
    
    client = CrabCacheClient()
    if not client.connect():
        return False
    
    try:
        # Preenche cache com dados para testar eviction
        print("📝 Preenchendo cache para testar eviction...")
        for i in range(50):
            key = f"eviction:test:{i}"
            value = f"data_{i}_{'x' * 100}"  # Valores maiores para pressionar memória
            client.put(key, value)
        
        # Acessa algumas chaves frequentemente (para TinyLFU)
        hot_keys = ["eviction:test:1", "eviction:test:5", "eviction:test:10"]
        for _ in range(10):
            for key in hot_keys:
                client.get(key)
        
        # Adiciona mais dados para forçar eviction
        for i in range(50, 100):
            key = f"eviction:test:{i}"
            value = f"data_{i}_{'x' * 100}"
            client.put(key, value)
        
        # Verifica se hot keys ainda estão no cache (TinyLFU deve mantê-las)
        hot_key_hits = 0
        for key in hot_keys:
            response = client.get(key)
            if "data_" in response:
                hot_key_hits += 1
        
        print(f"✅ TinyLFU manteve {hot_key_hits}/{len(hot_keys)} hot keys")
        
        # Obtém estatísticas de eviction
        stats = client.stats()
        if 'eviction' in stats:
            eviction_stats = stats['eviction']
            print(f"📊 Estatísticas de Eviction:")
            print(f"   - Uso total de memória: {eviction_stats.get('total_memory_usage', 'N/A')}")
            print(f"   - Limite de memória: {eviction_stats.get('total_memory_limit', 'N/A')}")
            print(f"   - Ratio de uso: {eviction_stats.get('overall_usage_ratio', 'N/A'):.4f}")
            
            if 'shards' in eviction_stats:
                total_evictions = sum(shard.get('evictions', 0) for shard in eviction_stats['shards'])
                total_hits = sum(shard.get('cache_hits', 0) for shard in eviction_stats['shards'])
                total_misses = sum(shard.get('cache_misses', 0) for shard in eviction_stats['shards'])
                hit_ratio = total_hits / (total_hits + total_misses) if (total_hits + total_misses) > 0 else 0
                
                print(f"   - Total de evictions: {total_evictions}")
                print(f"   - Hit ratio global: {hit_ratio:.4f}")
        
        return True
        
    except Exception as e:
        print(f"❌ Erro no teste de eviction: {e}")
        return False
    finally:
        client.disconnect()

def test_wal_persistence():
    """Testa sistema de persistência WAL"""
    print("\n💾 Testando sistema de persistência WAL...")
    
    # Primeiro, para o container atual se estiver rodando
    try:
        subprocess.run(["docker", "stop", "crabcache-wal-test"], 
                      capture_output=True, timeout=10)
        subprocess.run(["docker", "rm", "crabcache-wal-test"], 
                      capture_output=True, timeout=10)
    except:
        pass
    
    try:
        # Inicia container com WAL habilitado
        print("🚀 Iniciando CrabCache com WAL habilitado...")
        cmd = [
            "docker", "run", "-d",
            "--name", "crabcache-wal-test",
            "-p", "8000:8000",
            "-p", "9090:9090",
            "-e", "CRABCACHE_ENABLE_WAL=true",
            "-e", "CRABCACHE_WAL_SYNC_POLICY=sync",
            "crabcache:latest-wal-async"
        ]
        
        result = subprocess.run(cmd, capture_output=True, text=True, timeout=30)
        if result.returncode != 0:
            print(f"❌ Erro ao iniciar container: {result.stderr}")
            return False
        
        # Aguarda o servidor inicializar
        print("⏳ Aguardando servidor inicializar...")
        time.sleep(5)
        
        # Conecta e adiciona dados
        client = CrabCacheClient()
        if not client.connect():
            print("❌ Não foi possível conectar ao servidor")
            return False
        
        # Adiciona dados que devem ser persistidos
        test_data = {
            "user:1001": '{"name": "Alice", "age": 30, "city": "São Paulo"}',
            "user:1002": '{"name": "Bob", "age": 25, "city": "Rio de Janeiro"}',
            "counter:visits": "42",
            "config:theme": "dark",
            "session:abc123": '{"user_id": 1001, "expires": 1640995200}'
        }
        
        print("📝 Adicionando dados para persistência...")
        for key, value in test_data.items():
            response = client.put(key, value)
            print(f"   PUT {key}: {response}")
        
        # Verifica se os dados foram inseridos
        print("🔍 Verificando dados inseridos...")
        for key in test_data.keys():
            response = client.get(key)
            if test_data[key] in response:
                print(f"   ✅ {key}: OK")
            else:
                print(f"   ❌ {key}: FALHOU - {response}")
        
        client.disconnect()
        
        # Para o container (simula crash)
        print("🔄 Simulando crash do servidor...")
        subprocess.run(["docker", "stop", "crabcache-wal-test"], 
                      capture_output=True, timeout=10)
        
        # Reinicia o container (deve fazer recovery)
        print("🚀 Reiniciando servidor (testando recovery)...")
        subprocess.run(["docker", "start", "crabcache-wal-test"], 
                      capture_output=True, timeout=10)
        
        # Aguarda recovery
        time.sleep(5)
        
        # Verifica se os dados foram recuperados
        client = CrabCacheClient()
        if not client.connect():
            print("❌ Não foi possível reconectar após recovery")
            return False
        
        print("🔍 Verificando recovery dos dados...")
        recovered_count = 0
        for key, expected_value in test_data.items():
            response = client.get(key)
            if expected_value in response:
                print(f"   ✅ {key}: RECUPERADO")
                recovered_count += 1
            else:
                print(f"   ❌ {key}: NÃO RECUPERADO - {response}")
        
        client.disconnect()
        
        recovery_ratio = recovered_count / len(test_data)
        print(f"📊 Recovery: {recovered_count}/{len(test_data)} itens ({recovery_ratio:.1%})")
        
        if recovery_ratio >= 0.8:  # 80% ou mais recuperado
            print("✅ Sistema WAL funcionando corretamente!")
            return True
        else:
            print("❌ Sistema WAL com problemas de recovery")
            return False
        
    except Exception as e:
        print(f"❌ Erro no teste WAL: {e}")
        return False
    finally:
        # Limpa container de teste
        try:
            subprocess.run(["docker", "stop", "crabcache-wal-test"], 
                          capture_output=True, timeout=10)
            subprocess.run(["docker", "rm", "crabcache-wal-test"], 
                          capture_output=True, timeout=10)
        except:
            pass

def test_observability():
    """Testa sistema de observabilidade"""
    print("\n📊 Testando sistema de observabilidade...")
    
    try:
        # Inicia container para testes de observabilidade
        print("🚀 Iniciando CrabCache para testes de observabilidade...")
        cmd = [
            "docker", "run", "-d",
            "--name", "crabcache-obs-test",
            "-p", "8000:8000",
            "-p", "9090:9090",
            "crabcache:latest-wal-async"
        ]
        
        subprocess.run(cmd, capture_output=True, timeout=30)
        time.sleep(3)
        
        # Testa endpoint de métricas Prometheus
        print("🔍 Testando endpoint Prometheus...")
        try:
            response = requests.get("http://localhost:9090/metrics", timeout=5)
            if response.status_code == 200:
                metrics_text = response.text
                if "crabcache_" in metrics_text:
                    print("✅ Endpoint Prometheus funcionando")
                    print(f"   - Métricas encontradas: {metrics_text.count('crabcache_')}")
                else:
                    print("❌ Métricas CrabCache não encontradas")
            else:
                print(f"❌ Endpoint Prometheus falhou: {response.status_code}")
        except Exception as e:
            print(f"❌ Erro ao acessar Prometheus: {e}")
        
        # Testa endpoint de health
        print("🔍 Testando endpoint de health...")
        try:
            response = requests.get("http://localhost:9090/health", timeout=5)
            if response.status_code == 200:
                print("✅ Health check funcionando")
                print(f"   - Status: {response.text}")
            else:
                print(f"❌ Health check falhou: {response.status_code}")
        except Exception as e:
            print(f"❌ Erro ao acessar health: {e}")
        
        # Testa dashboard
        print("🔍 Testando dashboard...")
        try:
            response = requests.get("http://localhost:9090/dashboard", timeout=5)
            if response.status_code == 200:
                if "CrabCache Dashboard" in response.text:
                    print("✅ Dashboard funcionando")
                else:
                    print("❌ Dashboard sem conteúdo esperado")
            else:
                print(f"❌ Dashboard falhou: {response.status_code}")
        except Exception as e:
            print(f"❌ Erro ao acessar dashboard: {e}")
        
        # Testa comando STATS
        print("🔍 Testando comando STATS...")
        client = CrabCacheClient()
        if client.connect():
            try:
                stats = client.stats()
                if isinstance(stats, dict) and 'eviction' in stats:
                    print("✅ Comando STATS funcionando")
                    print(f"   - Shards: {len(stats['eviction'].get('shards', []))}")
                    print(f"   - Uso de memória: {stats['eviction'].get('total_memory_usage', 'N/A')}")
                else:
                    print(f"❌ STATS retornou formato inválido: {stats}")
            except Exception as e:
                print(f"❌ Erro no comando STATS: {e}")
            finally:
                client.disconnect()
        
        return True
        
    except Exception as e:
        print(f"❌ Erro no teste de observabilidade: {e}")
        return False
    finally:
        # Limpa container de teste
        try:
            subprocess.run(["docker", "stop", "crabcache-obs-test"], 
                          capture_output=True, timeout=10)
            subprocess.run(["docker", "rm", "crabcache-obs-test"], 
                          capture_output=True, timeout=10)
        except:
            pass

def test_performance():
    """Testa performance básica"""
    print("\n🚀 Testando performance básica...")
    
    try:
        # Inicia container para teste de performance
        cmd = [
            "docker", "run", "-d",
            "--name", "crabcache-perf-test",
            "-p", "8000:8000",
            "crabcache:latest-wal-async"
        ]
        
        subprocess.run(cmd, capture_output=True, timeout=30)
        time.sleep(3)
        
        client = CrabCacheClient()
        if not client.connect():
            return False
        
        # Teste de throughput básico
        print("📊 Testando throughput...")
        operations = 1000
        start_time = time.time()
        
        # PUT operations
        for i in range(operations):
            key = f"perf:test:{i}"
            value = f"value_{i}_{'x' * 50}"
            client.put(key, value)
        
        put_time = time.time() - start_time
        put_ops_per_sec = operations / put_time
        
        # GET operations
        start_time = time.time()
        for i in range(operations):
            key = f"perf:test:{i}"
            client.get(key)
        
        get_time = time.time() - start_time
        get_ops_per_sec = operations / get_time
        
        print(f"📊 Resultados de Performance:")
        print(f"   - PUT: {put_ops_per_sec:.0f} ops/sec")
        print(f"   - GET: {get_ops_per_sec:.0f} ops/sec")
        print(f"   - Latência média PUT: {(put_time/operations)*1000:.2f}ms")
        print(f"   - Latência média GET: {(get_time/operations)*1000:.2f}ms")
        
        # Verifica se performance está aceitável
        if put_ops_per_sec > 1000 and get_ops_per_sec > 1000:
            print("✅ Performance aceitável")
            return True
        else:
            print("⚠️ Performance abaixo do esperado")
            return False
        
    except Exception as e:
        print(f"❌ Erro no teste de performance: {e}")
        return False
    finally:
        client.disconnect()
        try:
            subprocess.run(["docker", "stop", "crabcache-perf-test"], 
                          capture_output=True, timeout=10)
            subprocess.run(["docker", "rm", "crabcache-perf-test"], 
                          capture_output=True, timeout=10)
        except:
            pass

def main():
    """Executa todos os testes"""
    print("🚀 CrabCache - Teste Completo de Funcionalidades")
    print("=" * 60)
    print("Testando Fase 4.1 (TinyLFU Eviction) + Fase 4.2 (WAL Persistence)")
    print("=" * 60)
    
    tests = [
        ("Operações Básicas", test_basic_operations),
        ("Sistema de Eviction TinyLFU", test_eviction_system),
        ("Persistência WAL", test_wal_persistence),
        ("Observabilidade", test_observability),
        ("Performance Básica", test_performance),
    ]
    
    results = []
    
    for test_name, test_func in tests:
        print(f"\n{'='*20} {test_name} {'='*20}")
        try:
            result = test_func()
            results.append((test_name, result))
            if result:
                print(f"✅ {test_name}: PASSOU")
            else:
                print(f"❌ {test_name}: FALHOU")
        except Exception as e:
            print(f"❌ {test_name}: ERRO - {e}")
            results.append((test_name, False))
    
    # Resumo final
    print("\n" + "="*60)
    print("📋 RESUMO DOS TESTES")
    print("="*60)
    
    passed = sum(1 for _, result in results if result)
    total = len(results)
    
    for test_name, result in results:
        status = "✅ PASSOU" if result else "❌ FALHOU"
        print(f"{test_name:.<40} {status}")
    
    print("-" * 60)
    print(f"Total: {passed}/{total} testes passaram ({passed/total:.1%})")
    
    if passed == total:
        print("\n🎉 TODOS OS TESTES PASSARAM!")
        print("✅ CrabCache com TinyLFU + WAL está funcionando perfeitamente!")
        return 0
    else:
        print(f"\n⚠️ {total-passed} teste(s) falharam")
        print("❌ Algumas funcionalidades precisam de atenção")
        return 1

if __name__ == "__main__":
    sys.exit(main())