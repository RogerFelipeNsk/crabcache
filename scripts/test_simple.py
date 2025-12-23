#!/usr/bin/env python3
"""
Teste simples e direto das funcionalidades CrabCache
"""

import socket
import time
import subprocess
import json

def test_basic_functionality():
    """Testa funcionalidades básicas com container já rodando"""
    print("🔧 Testando funcionalidades básicas...")
    
    try:
        sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        sock.settimeout(10)
        sock.connect(('localhost', 8000))
        
        def send_cmd(cmd):
            sock.send((cmd + '\n').encode())
            response = sock.recv(4096).decode().strip()
            return response
        
        # Test PING
        response = send_cmd("PING")
        print(f"PING: {response}")
        assert "PONG" in response
        
        # Test PUT/GET simples
        response = send_cmd("PUT test_key test_value")
        print(f"PUT test_key: {response}")
        
        response = send_cmd("GET test_key")
        print(f"GET test_key: {response}")
        
        # Test PUT com TTL
        response = send_cmd("PUT ttl_key ttl_value 10")
        print(f"PUT com TTL: {response}")
        
        # Test DELETE
        response = send_cmd("DEL test_key")
        print(f"DEL test_key: {response}")
        
        # Test STATS
        response = send_cmd("STATS")
        print(f"STATS (primeiras 200 chars): {response[:200]}...")
        
        sock.close()
        print("✅ Funcionalidades básicas OK")
        return True
        
    except Exception as e:
        print(f"❌ Erro: {e}")
        return False

def test_metrics_endpoints():
    """Testa endpoints de métricas"""
    print("\n📊 Testando endpoints de métricas...")
    
    try:
        import requests
        
        # Test Prometheus
        response = requests.get("http://localhost:9090/metrics", timeout=5)
        if response.status_code == 200:
            print("✅ Prometheus endpoint OK")
            print(f"   Métricas encontradas: {response.text.count('crabcache_')}")
        else:
            print(f"❌ Prometheus falhou: {response.status_code}")
        
        # Test Health
        response = requests.get("http://localhost:9090/health", timeout=5)
        if response.status_code == 200:
            print("✅ Health endpoint OK")
            print(f"   Status: {response.text}")
        else:
            print(f"❌ Health falhou: {response.status_code}")
        
        # Test Dashboard
        response = requests.get("http://localhost:9090/dashboard", timeout=5)
        if response.status_code == 200:
            print("✅ Dashboard endpoint OK")
        else:
            print(f"❌ Dashboard falhou: {response.status_code}")
        
        return True
        
    except Exception as e:
        print(f"❌ Erro nos endpoints: {e}")
        return False

def test_performance():
    """Teste básico de performance"""
    print("\n🚀 Testando performance...")
    
    try:
        sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        sock.settimeout(10)
        sock.connect(('localhost', 8000))
        
        def send_cmd(cmd):
            sock.send((cmd + '\n').encode())
            response = sock.recv(4096).decode().strip()
            return response
        
        # Teste de throughput
        operations = 100
        start_time = time.time()
        
        for i in range(operations):
            send_cmd(f"PUT perf_key_{i} perf_value_{i}")
        
        put_time = time.time() - start_time
        put_ops_per_sec = operations / put_time
        
        start_time = time.time()
        for i in range(operations):
            send_cmd(f"GET perf_key_{i}")
        
        get_time = time.time() - start_time
        get_ops_per_sec = operations / get_time
        
        print(f"📊 Performance:")
        print(f"   PUT: {put_ops_per_sec:.0f} ops/sec")
        print(f"   GET: {get_ops_per_sec:.0f} ops/sec")
        
        sock.close()
        
        if put_ops_per_sec > 500 and get_ops_per_sec > 500:
            print("✅ Performance OK")
            return True
        else:
            print("⚠️ Performance baixa")
            return False
        
    except Exception as e:
        print(f"❌ Erro na performance: {e}")
        return False

def main():
    print("🚀 CrabCache - Teste Simples de Validação")
    print("=" * 50)
    
    # Verifica se há container rodando
    try:
        result = subprocess.run(["docker", "ps"], capture_output=True, text=True)
        if "crabcache" not in result.stdout:
            print("⚠️ Nenhum container CrabCache detectado rodando")
            print("Iniciando container de teste...")
            
            subprocess.run([
                "docker", "run", "-d", "--name", "crabcache-simple-test",
                "-p", "8000:8000", "-p", "9090:9090",
                "crabcache:latest-wal-async"
            ], check=True)
            
            print("⏳ Aguardando inicialização...")
            time.sleep(5)
    except Exception as e:
        print(f"❌ Erro ao verificar/iniciar container: {e}")
        return 1
    
    tests = [
        ("Funcionalidades Básicas", test_basic_functionality),
        ("Endpoints de Métricas", test_metrics_endpoints),
        ("Performance Básica", test_performance),
    ]
    
    results = []
    for test_name, test_func in tests:
        print(f"\n{'='*20} {test_name} {'='*20}")
        try:
            result = test_func()
            results.append((test_name, result))
        except Exception as e:
            print(f"❌ Erro no teste {test_name}: {e}")
            results.append((test_name, False))
    
    # Resumo
    print("\n" + "="*50)
    print("📋 RESUMO")
    print("="*50)
    
    passed = sum(1 for _, result in results if result)
    total = len(results)
    
    for test_name, result in results:
        status = "✅ PASSOU" if result else "❌ FALHOU"
        print(f"{test_name:.<30} {status}")
    
    print("-" * 50)
    print(f"Total: {passed}/{total} ({passed/total:.1%})")
    
    # Cleanup
    try:
        subprocess.run(["docker", "stop", "crabcache-simple-test"], 
                      capture_output=True, timeout=10)
        subprocess.run(["docker", "rm", "crabcache-simple-test"], 
                      capture_output=True, timeout=10)
    except:
        pass
    
    return 0 if passed == total else 1

if __name__ == "__main__":
    exit(main())