#!/usr/bin/env python3
"""
Script de debug para identificar problemas no CrabCache
"""

import socket
import time
import threading
import random

def test_single_connection(host='localhost', port=7001):
    """Testa uma única conexão com várias operações"""
    print("🔍 Testando conexão única...")
    
    try:
        sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        sock.connect((host, port))
        
        operations = [
            ("PING", ""),
            ("PUT test_key test_value", ""),
            ("GET test_key", ""),
            ("STATS", ""),
            ("DEL test_key", ""),
            ("GET test_key", ""),  # Deve falhar
        ]
        
        for i, (cmd, expected) in enumerate(operations):
            print(f"  {i+1}. Enviando: {cmd}")
            sock.send((cmd + '\n').encode())
            response = sock.recv(1024).decode().strip()
            print(f"     Resposta: {response}")
            time.sleep(0.1)
        
        sock.close()
        print("✅ Teste de conexão única concluído")
        
    except Exception as e:
        print(f"❌ Erro no teste de conexão única: {e}")

def test_concurrent_connections(host='localhost', port=7001, num_connections=5):
    """Testa múltiplas conexões simultâneas"""
    print(f"🔍 Testando {num_connections} conexões simultâneas...")
    
    results = []
    
    def worker(worker_id):
        try:
            sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
            sock.connect((host, port))
            
            success_count = 0
            total_ops = 10
            
            for i in range(total_ops):
                key = f"worker_{worker_id}_key_{i}"
                value = f"worker_{worker_id}_value_{i}"
                
                # PUT
                sock.send(f"PUT {key} {value}\n".encode())
                response = sock.recv(1024).decode().strip()
                if "OK" in response:
                    success_count += 1
                
                # GET
                sock.send(f"GET {key}\n".encode())
                response = sock.recv(1024).decode().strip()
                if value in response:
                    success_count += 1
                
                time.sleep(0.01)  # Pequena pausa
            
            sock.close()
            results.append((worker_id, success_count, total_ops * 2))
            print(f"  Worker {worker_id}: {success_count}/{total_ops * 2} sucessos")
            
        except Exception as e:
            print(f"❌ Worker {worker_id} falhou: {e}")
            results.append((worker_id, 0, 0))
    
    threads = []
    for i in range(num_connections):
        t = threading.Thread(target=worker, args=(i,))
        threads.append(t)
        t.start()
    
    for t in threads:
        t.join()
    
    total_success = sum(r[1] for r in results)
    total_ops = sum(r[2] for r in results)
    success_rate = (total_success / total_ops * 100) if total_ops > 0 else 0
    
    print(f"📊 Resultado: {total_success}/{total_ops} ({success_rate:.1f}% sucesso)")

def test_rapid_fire(host='localhost', port=7001, ops_count=100):
    """Testa operações rápidas em sequência"""
    print(f"🔍 Testando {ops_count} operações rápidas...")
    
    try:
        sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        sock.connect((host, port))
        
        success_count = 0
        start_time = time.time()
        
        for i in range(ops_count):
            key = f"rapid_key_{i}"
            value = f"rapid_value_{i}"
            
            # Enviar PUT
            sock.send(f"PUT {key} {value}\n".encode())
            response = sock.recv(1024).decode().strip()
            
            if "OK" in response:
                success_count += 1
            else:
                print(f"❌ PUT falhou para {key}: {response}")
        
        elapsed = time.time() - start_time
        rate = ops_count / elapsed
        success_rate = (success_count / ops_count * 100)
        
        print(f"📊 Resultado: {success_count}/{ops_count} ({success_rate:.1f}% sucesso)")
        print(f"⚡ Taxa: {rate:.1f} ops/sec")
        
        sock.close()
        
    except Exception as e:
        print(f"❌ Erro no teste rapid fire: {e}")

def test_connection_reuse(host='localhost', port=7001):
    """Testa reutilização de conexão"""
    print("🔍 Testando reutilização de conexão...")
    
    try:
        sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        sock.connect((host, port))
        
        success_count = 0
        total_ops = 50
        
        for i in range(total_ops):
            # Alternar entre diferentes operações
            if i % 4 == 0:
                cmd = "PING"
            elif i % 4 == 1:
                cmd = f"PUT reuse_key_{i} reuse_value_{i}"
            elif i % 4 == 2:
                cmd = f"GET reuse_key_{i-1}"
            else:
                cmd = "STATS"
            
            sock.send((cmd + '\n').encode())
            response = sock.recv(1024).decode().strip()
            
            if ("PONG" in response or "OK" in response or 
                "reuse_value" in response or "total_keys" in response):
                success_count += 1
            else:
                print(f"❌ Falha na operação {i}: {cmd} -> {response}")
        
        success_rate = (success_count / total_ops * 100)
        print(f"📊 Resultado: {success_count}/{total_ops} ({success_rate:.1f}% sucesso)")
        
        sock.close()
        
    except Exception as e:
        print(f"❌ Erro no teste de reutilização: {e}")

if __name__ == "__main__":
    print("🚀 Debug do CrabCache - Identificando Problemas")
    print("=" * 50)
    
    # Teste 1: Conexão única
    test_single_connection()
    print()
    
    # Teste 2: Conexões simultâneas (baixa concorrência)
    test_concurrent_connections(num_connections=3)
    print()
    
    # Teste 3: Operações rápidas
    test_rapid_fire(ops_count=50)
    print()
    
    # Teste 4: Reutilização de conexão
    test_connection_reuse()
    print()
    
    # Teste 5: Conexões simultâneas (alta concorrência)
    test_concurrent_connections(num_connections=10)
    print()
    
    print("🏁 Debug concluído!")