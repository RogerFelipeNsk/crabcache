#!/usr/bin/env python3
"""
Teste focado no sistema WAL de persistência
"""

import socket
import time
import subprocess
import os

def test_wal_persistence():
    """Testa persistência WAL com recovery"""
    print("💾 Testando sistema WAL...")
    
    # Limpa containers anteriores
    subprocess.run(["docker", "stop", "crabcache-wal-test"], capture_output=True)
    subprocess.run(["docker", "rm", "crabcache-wal-test"], capture_output=True)
    
    try:
        # Fase 1: Inicia container com WAL habilitado
        print("🚀 Iniciando CrabCache com WAL habilitado...")
        cmd = [
            "docker", "run", "-d",
            "--name", "crabcache-wal-test",
            "-p", "8000:8000",
            "-e", "CRABCACHE_ENABLE_WAL=true",
            "-e", "CRABCACHE_WAL_SYNC_POLICY=sync",
            "-v", "/tmp/crabcache-wal:/app/data/wal",  # Volume para persistir WAL
            "crabcache:latest-wal-async"
        ]
        
        result = subprocess.run(cmd, capture_output=True, text=True, timeout=30)
        if result.returncode != 0:
            print(f"❌ Erro ao iniciar container: {result.stderr}")
            return False
        
        print("⏳ Aguardando inicialização...")
        time.sleep(5)
        
        # Fase 2: Adiciona dados
        print("📝 Adicionando dados...")
        sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        sock.settimeout(10)
        sock.connect(('localhost', 8000))
        
        def send_cmd(cmd):
            sock.send((cmd + '\n').encode())
            response = sock.recv(4096).decode().strip()
            return response
        
        # Adiciona dados simples (sem JSON complexo)
        test_data = {
            "user1": "alice",
            "user2": "bob", 
            "counter": "42",
            "config": "dark_theme",
            "session": "abc123"
        }
        
        for key, value in test_data.items():
            response = send_cmd(f"PUT {key} {value}")
            print(f"   PUT {key}: {response}")
        
        # Verifica se dados foram inseridos
        print("🔍 Verificando dados inseridos...")
        for key, expected_value in test_data.items():
            response = send_cmd(f"GET {key}")
            if expected_value in response:
                print(f"   ✅ {key}: {response}")
            else:
                print(f"   ❌ {key}: {response}")
        
        sock.close()
        
        # Fase 3: Para container (simula crash)
        print("🔄 Simulando crash...")
        subprocess.run(["docker", "stop", "crabcache-wal-test"], timeout=10)
        
        # Fase 4: Reinicia container (recovery)
        print("🚀 Reiniciando para testar recovery...")
        subprocess.run(["docker", "start", "crabcache-wal-test"], timeout=10)
        time.sleep(5)
        
        # Fase 5: Verifica recovery
        print("🔍 Verificando recovery...")
        sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        sock.settimeout(10)
        sock.connect(('localhost', 8000))
        
        recovered_count = 0
        for key, expected_value in test_data.items():
            response = send_cmd(f"GET {key}")
            if expected_value in response:
                print(f"   ✅ {key}: RECUPERADO ({response})")
                recovered_count += 1
            else:
                print(f"   ❌ {key}: NÃO RECUPERADO ({response})")
        
        sock.close()
        
        recovery_ratio = recovered_count / len(test_data)
        print(f"📊 Recovery: {recovered_count}/{len(test_data)} ({recovery_ratio:.1%})")
        
        return recovery_ratio >= 0.8  # 80% ou mais
        
    except Exception as e:
        print(f"❌ Erro no teste WAL: {e}")
        return False
    finally:
        # Cleanup
        subprocess.run(["docker", "stop", "crabcache-wal-test"], capture_output=True)
        subprocess.run(["docker", "rm", "crabcache-wal-test"], capture_output=True)

def test_wal_disabled():
    """Testa funcionamento com WAL desabilitado"""
    print("\n💾 Testando com WAL desabilitado...")
    
    subprocess.run(["docker", "stop", "crabcache-no-wal"], capture_output=True)
    subprocess.run(["docker", "rm", "crabcache-no-wal"], capture_output=True)
    
    try:
        # Inicia sem WAL
        cmd = [
            "docker", "run", "-d",
            "--name", "crabcache-no-wal",
            "-p", "8001:8000",  # Porta diferente
            "-e", "CRABCACHE_ENABLE_WAL=false",
            "crabcache:latest-wal-async"
        ]
        
        subprocess.run(cmd, check=True, timeout=30)
        time.sleep(3)
        
        # Testa operações básicas
        sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        sock.settimeout(5)
        sock.connect(('localhost', 8001))
        
        def send_cmd(cmd):
            sock.send((cmd + '\n').encode())
            response = sock.recv(4096).decode().strip()
            return response
        
        # Testa algumas operações
        response = send_cmd("PING")
        assert "PONG" in response
        
        response = send_cmd("PUT test_no_wal value_no_wal")
        print(f"PUT sem WAL: {response}")
        
        response = send_cmd("GET test_no_wal")
        print(f"GET sem WAL: {response}")
        
        sock.close()
        print("✅ Funcionamento sem WAL OK")
        return True
        
    except Exception as e:
        print(f"❌ Erro no teste sem WAL: {e}")
        return False
    finally:
        subprocess.run(["docker", "stop", "crabcache-no-wal"], capture_output=True)
        subprocess.run(["docker", "rm", "crabcache-no-wal"], capture_output=True)

def main():
    print("🚀 CrabCache - Teste Focado em WAL Persistence")
    print("=" * 60)
    
    tests = [
        ("WAL Persistence e Recovery", test_wal_persistence),
        ("Funcionamento sem WAL", test_wal_disabled),
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
    
    # Resumo
    print("\n" + "="*60)
    print("📋 RESUMO DOS TESTES WAL")
    print("="*60)
    
    passed = sum(1 for _, result in results if result)
    total = len(results)
    
    for test_name, result in results:
        status = "✅ PASSOU" if result else "❌ FALHOU"
        print(f"{test_name:.<40} {status}")
    
    print("-" * 60)
    print(f"Total: {passed}/{total} ({passed/total:.1%})")
    
    if passed == total:
        print("\n🎉 TODOS OS TESTES WAL PASSARAM!")
        print("✅ Sistema de persistência WAL funcionando!")
    else:
        print(f"\n⚠️ {total-passed} teste(s) WAL falharam")
    
    return 0 if passed == total else 1

if __name__ == "__main__":
    exit(main())