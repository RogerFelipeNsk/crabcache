#!/usr/bin/env python3
import socket
import time

def test_connection():
    try:
        sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        sock.settimeout(5)
        result = sock.connect_ex(('localhost', 8000))  # Porta correta
        if result == 0:
            print("✅ Conexão TCP na porta 8000 funcionando")
            
            # Testa PING
            sock.send(b'PING\n')
            response = sock.recv(1024)
            print(f"📡 Resposta: {response.decode().strip()}")
            
            sock.close()
            return True
        else:
            print(f"❌ Não foi possível conectar na porta 8000: {result}")
            return False
    except Exception as e:
        print(f"❌ Erro na conexão: {e}")
        return False

if __name__ == "__main__":
    print("🔍 Testando conexão com CrabCache...")
    test_connection()