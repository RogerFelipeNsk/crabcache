#!/usr/bin/env python3
"""
Script para investigar erros específicos no CrabCache
"""

import socket
import time
import threading
from concurrent.futures import ThreadPoolExecutor, as_completed

def test_single_operation(host='localhost', port=7001, operation="GET test_key"):
    """Testa uma operação específica"""
    try:
        sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        sock.settimeout(5.0)  # Timeout de 5 segundos
        sock.connect((host, port))
        
        sock.send((operation + '\n').encode())
        response = sock.recv(1024).decode().strip()
        
        sock.close()
        return True, response
    except Exception as e:
        return False, str(e)

def stress_test_connections(host='localhost', port=7001, num_connections=20, ops_per_conn=10):
    """Testa muitas conexões simultâneas"""
    print(f"🔥 Teste de stress: {num_connections} conexões, {ops_per_conn} ops cada")
    
    results = []
    errors = []
    
    def worker(worker_id):
        worker_results = []
        worker_errors = []
        
        try:
            sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
            sock.settimeout(10.0)
            sock.connect((host, port))
            
            for i in range(ops_per_conn):
                try:
                    # Operação simples
                    cmd = f"PUT stress_{worker_id}_{i} value_{worker_id}_{i}"
                    sock.send((cmd + '\n').encode())
                    response = sock.recv(1024).decode().strip()
                    
                    if response == "OK":
                        worker_results.append(("PUT", True, response))
                    else:
                        worker_results.append(("PUT", False, response))
                        worker_errors.append(f"Worker {worker_id} PUT {i}: {response}")
                    
                    time.sleep(0.01)  # Pequena pausa
                    
                except Exception as e:
                    worker_errors.append(f"Worker {worker_id} op {i}: {e}")
                    worker_results.append(("PUT", False, str(e)))
            
            sock.close()
            
        except Exception as e:
            worker_errors.append(f"Worker {worker_id} connection: {e}")
        
        return worker_results, worker_errors
    
    start_time = time.time()
    
    with ThreadPoolExecutor(max_workers=num_connections) as executor:
        futures = [executor.submit(worker, i) for i in range(num_connections)]
        
        for future in as_completed(futures):
            try:
                worker_results, worker_errors = future.result()
                results.extend(worker_results)
                errors.extend(worker_errors)
            except Exception as e:
                errors.append(f"Future error: {e}")
    
    elapsed = time.time() - start_time
    
    total_ops = len(results)
    successful_ops = sum(1 for _, success, _ in results if success)
    success_rate = (successful_ops / total_ops * 100) if total_ops > 0 else 0
    
    print(f"📊 Resultados:")
    print(f"   Total operações: {total_ops}")
    print(f"   Sucessos: {successful_ops}")
    print(f"   Taxa de sucesso: {success_rate:.1f}%")
    print(f"   Tempo: {elapsed:.2f}s")
    print(f"   Throughput: {total_ops/elapsed:.1f} ops/sec")
    
    if errors:
        print(f"\n❌ Erros encontrados ({len(errors)}):")
        for error in errors[:10]:  # Mostrar apenas os primeiros 10
            print(f"   {error}")
        if len(errors) > 10:
            print(f"   ... e mais {len(errors) - 10} erros")
    
    return success_rate, errors

def test_connection_limits(host='localhost', port=7001):
    """Testa limites de conexão"""
    print("🔍 Testando limites de conexão...")
    
    connections = []
    max_connections = 0
    
    try:
        for i in range(200):  # Tentar até 200 conexões
            sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
            sock.settimeout(2.0)
            sock.connect((host, port))
            connections.append(sock)
            max_connections = i + 1
            
            if i % 10 == 0:
                print(f"   {i+1} conexões abertas...")
    
    except Exception as e:
        print(f"❌ Limite atingido em {max_connections} conexões: {e}")
    
    finally:
        # Fechar todas as conexões
        for sock in connections:
            try:
                sock.close()
            except:
                pass
    
    print(f"📊 Máximo de conexões simultâneas: {max_connections}")
    return max_connections

def test_rapid_operations(host='localhost', port=7001, ops_count=1000):
    """Testa operações muito rápidas em uma conexão"""
    print(f"⚡ Teste de operações rápidas: {ops_count} ops")
    
    try:
        sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        sock.settimeout(30.0)
        sock.connect((host, port))
        
        success_count = 0
        errors = []
        start_time = time.time()
        
        for i in range(ops_count):
            try:
                cmd = f"PUT rapid_{i} value_{i}"
                sock.send((cmd + '\n').encode())
                response = sock.recv(1024).decode().strip()
                
                if response == "OK":
                    success_count += 1
                else:
                    errors.append(f"Op {i}: {response}")
                
            except Exception as e:
                errors.append(f"Op {i}: {e}")
        
        elapsed = time.time() - start_time
        success_rate = (success_count / ops_count * 100)
        
        print(f"📊 Resultados:")
        print(f"   Sucessos: {success_count}/{ops_count} ({success_rate:.1f}%)")
        print(f"   Tempo: {elapsed:.2f}s")
        print(f"   Throughput: {ops_count/elapsed:.1f} ops/sec")
        
        if errors:
            print(f"❌ Primeiros erros:")
            for error in errors[:5]:
                print(f"   {error}")
        
        sock.close()
        return success_rate
        
    except Exception as e:
        print(f"❌ Erro geral: {e}")
        return 0

def main():
    print("🚀 Investigação de Erros do CrabCache")
    print("=" * 50)
    
    # Teste 1: Operação simples
    print("\n1. Teste de operação simples:")
    success, response = test_single_operation()
    print(f"   Resultado: {'✅' if success else '❌'} {response}")
    
    # Teste 2: Limites de conexão
    print("\n2. Teste de limites de conexão:")
    max_conn = test_connection_limits()
    
    # Teste 3: Operações rápidas
    print("\n3. Teste de operações rápidas:")
    rapid_success = test_rapid_operations(ops_count=500)
    
    # Teste 4: Stress test com diferentes níveis
    print("\n4. Testes de stress:")
    
    for connections in [5, 10, 20]:
        print(f"\n   Teste com {connections} conexões:")
        success_rate, errors = stress_test_connections(
            num_connections=connections, 
            ops_per_conn=20
        )
        
        if success_rate < 90:
            print(f"   ⚠️  Taxa baixa com {connections} conexões!")
            break
    
    print("\n" + "=" * 50)
    print("🏁 Investigação concluída!")
    
    # Resumo
    print("\n📋 Resumo:")
    print(f"   Máximo de conexões: {max_conn}")
    print(f"   Operações rápidas: {rapid_success:.1f}% sucesso")
    print("\n💡 Recomendações:")
    
    if max_conn < 50:
        print("   - Servidor pode ter limite baixo de conexões")
    if rapid_success < 95:
        print("   - Problemas com operações rápidas sequenciais")
    
    print("   - Considere implementar connection pooling")
    print("   - Adicione logs detalhados no servidor")
    print("   - Monitore uso de recursos (CPU/memória)")

if __name__ == "__main__":
    main()