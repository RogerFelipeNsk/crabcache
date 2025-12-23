#!/usr/bin/env python3
"""
Script para popular o CrabCache com dados de teste
"""

import socket
import time
import sys

def send_command(sock, command):
    """Envia comando e recebe resposta"""
    try:
        sock.send(command.encode() + b'\n')
        response = sock.recv(1024).decode().strip()
        return response
    except Exception as e:
        return f"ERROR: {e}"

def populate_crabcache(host='localhost', port=7001, num_keys=1000):
    """Popula o CrabCache com chaves de teste"""
    
    print(f"🚀 Populando CrabCache em {host}:{port}")
    print(f"📊 Inserindo {num_keys} chaves...")
    
    try:
        sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        sock.connect((host, port))
        
        # Testar conectividade
        response = send_command(sock, "PING")
        if "PONG" not in response:
            print(f"❌ Erro na conectividade: {response}")
            return False
        
        print("✅ Conectividade OK")
        
        # Popular com chaves
        success_count = 0
        start_time = time.time()
        
        for i in range(num_keys):
            key = f"test_key_{i:06d}"
            value = f"test_value_{i:06d}_{'x' * 50}"  # Valor com ~60 bytes
            
            command = f"PUT {key} {value}"
            response = send_command(sock, command)
            
            if "OK" in response:
                success_count += 1
            else:
                print(f"❌ Falha ao inserir {key}: {response}")
            
            # Progress update
            if (i + 1) % 100 == 0:
                elapsed = time.time() - start_time
                rate = (i + 1) / elapsed
                print(f"📈 Progresso: {i+1}/{num_keys} ({success_count} sucessos) - {rate:.1f} ops/sec")
        
        elapsed = time.time() - start_time
        final_rate = success_count / elapsed
        
        print(f"\n✅ População concluída!")
        print(f"📊 Estatísticas:")
        print(f"   Total inserido: {success_count}/{num_keys}")
        print(f"   Taxa de sucesso: {success_count/num_keys*100:.1f}%")
        print(f"   Tempo total: {elapsed:.2f}s")
        print(f"   Taxa média: {final_rate:.1f} ops/sec")
        
        # Verificar algumas chaves
        print(f"\n🔍 Verificando chaves inseridas...")
        test_keys = [f"test_key_{i:06d}" for i in [0, num_keys//4, num_keys//2, num_keys-1]]
        
        for key in test_keys:
            response = send_command(sock, f"GET {key}")
            if "test_value" in response:
                print(f"✅ {key}: OK")
            else:
                print(f"❌ {key}: {response}")
        
        sock.close()
        return True
        
    except Exception as e:
        print(f"❌ Erro: {e}")
        return False

if __name__ == "__main__":
    import argparse
    
    parser = argparse.ArgumentParser(description='Popular CrabCache com dados de teste')
    parser.add_argument('--host', default='localhost', help='Host do CrabCache')
    parser.add_argument('--port', type=int, default=7001, help='Porta do CrabCache')
    parser.add_argument('--keys', type=int, default=1000, help='Número de chaves para inserir')
    
    args = parser.parse_args()
    
    success = populate_crabcache(args.host, args.port, args.keys)
    sys.exit(0 if success else 1)