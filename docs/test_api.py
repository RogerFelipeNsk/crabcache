#!/usr/bin/env python3
"""
Script de teste completo da API CrabCache
Testa todos os comandos disponíveis e valida as respostas
"""

import socket
import time
import sys
import json
from typing import Optional, Tuple

class CrabCacheClient:
    def __init__(self, host: str = 'localhost', port: int = 7000):
        self.host = host
        self.port = port
    
    def send_command(self, command: str) -> str:
        """Envia um comando e retorna a resposta"""
        try:
            sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
            sock.settimeout(5)
            sock.connect((self.host, self.port))
            
            # Adiciona \r\n se não estiver presente
            if not command.endswith('\r\n'):
                command += '\r\n'
            
            sock.send(command.encode())
            response = sock.recv(4096).decode().strip()
            sock.close()
            
            return response
        except Exception as e:
            return f"ERROR: {e}"

def test_ping(client: CrabCacheClient) -> bool:
    """Testa o comando PING"""
    print("🏓 Testando PING...")
    response = client.send_command("PING")
    success = response == "PONG"
    print(f"   Comando: PING")
    print(f"   Resposta: {response}")
    print(f"   Status: {'✅ PASS' if success else '❌ FAIL'}")
    return success

def test_put_get_del(client: CrabCacheClient) -> bool:
    """Testa operações básicas PUT/GET/DEL"""
    print("\n📦 Testando PUT/GET/DEL...")
    
    # Test 1: PUT simples
    print("   Test 1: PUT simples")
    response = client.send_command("PUT test_key test_value")
    if response != "OK":
        print(f"   ❌ PUT falhou: {response}")
        return False
    print(f"   ✅ PUT: {response}")
    
    # Test 2: GET do valor
    print("   Test 2: GET do valor")
    response = client.send_command("GET test_key")
    if response != "test_value":
        print(f"   ❌ GET falhou: esperado 'test_value', recebido '{response}'")
        return False
    print(f"   ✅ GET: {response}")
    
    # Test 3: DEL da chave
    print("   Test 3: DEL da chave")
    response = client.send_command("DEL test_key")
    if response != "OK":
        print(f"   ❌ DEL falhou: {response}")
        return False
    print(f"   ✅ DEL: {response}")
    
    # Test 4: GET após DEL (deve retornar NULL)
    print("   Test 4: GET após DEL")
    response = client.send_command("GET test_key")
    if response != "NULL":
        print(f"   ❌ GET após DEL falhou: esperado 'NULL', recebido '{response}'")
        return False
    print(f"   ✅ GET após DEL: {response}")
    
    return True

def test_put_with_ttl(client: CrabCacheClient) -> bool:
    """Testa PUT com TTL"""
    print("\n⏰ Testando PUT com TTL...")
    
    # PUT com TTL de 2 segundos
    print("   PUT com TTL de 2 segundos")
    response = client.send_command("PUT ttl_key ttl_value 2")
    if response != "OK":
        print(f"   ❌ PUT com TTL falhou: {response}")
        return False
    print(f"   ✅ PUT com TTL: {response}")
    
    # GET imediato (deve funcionar)
    print("   GET imediato")
    response = client.send_command("GET ttl_key")
    if response != "ttl_value":
        print(f"   ❌ GET imediato falhou: {response}")
        return False
    print(f"   ✅ GET imediato: {response}")
    
    # Aguardar expiração
    print("   Aguardando 3 segundos para expiração...")
    time.sleep(3)
    
    # GET após expiração (deve retornar NULL)
    print("   GET após expiração")
    response = client.send_command("GET ttl_key")
    if response != "NULL":
        print(f"   ❌ GET após expiração falhou: esperado 'NULL', recebido '{response}'")
        return False
    print(f"   ✅ GET após expiração: {response}")
    
    return True

def test_expire_command(client: CrabCacheClient) -> bool:
    """Testa o comando EXPIRE"""
    print("\n⏳ Testando comando EXPIRE...")
    
    # PUT sem TTL
    print("   PUT sem TTL")
    response = client.send_command("PUT expire_key expire_value")
    if response != "OK":
        print(f"   ❌ PUT falhou: {response}")
        return False
    print(f"   ✅ PUT: {response}")
    
    # EXPIRE com 2 segundos
    print("   EXPIRE com 2 segundos")
    response = client.send_command("EXPIRE expire_key 2")
    if response != "OK":
        print(f"   ❌ EXPIRE falhou: {response}")
        return False
    print(f"   ✅ EXPIRE: {response}")
    
    # GET imediato
    print("   GET imediato")
    response = client.send_command("GET expire_key")
    if response != "expire_value":
        print(f"   ❌ GET falhou: {response}")
        return False
    print(f"   ✅ GET: {response}")
    
    # Aguardar expiração
    print("   Aguardando 3 segundos...")
    time.sleep(3)
    
    # GET após expiração
    print("   GET após expiração")
    response = client.send_command("GET expire_key")
    if response != "NULL":
        print(f"   ❌ GET após expiração falhou: {response}")
        return False
    print(f"   ✅ GET após expiração: {response}")
    
    return True

def test_json_data(client: CrabCacheClient) -> bool:
    """Testa armazenamento de dados JSON"""
    print("\n📄 Testando dados JSON...")
    
    # JSON sem espaços para evitar problemas de parsing
    json_data = '{"name":"John","age":30,"active":true}'
    
    # PUT JSON
    print("   PUT dados JSON")
    response = client.send_command(f"PUT user:profile {json_data}")
    if response != "OK":
        print(f"   ❌ PUT JSON falhou: {response}")
        return False
    print(f"   ✅ PUT JSON: {response}")
    
    # GET JSON
    print("   GET dados JSON")
    response = client.send_command("GET user:profile")
    if response != json_data:
        print(f"   ❌ GET JSON falhou: esperado '{json_data}', recebido '{response}'")
        return False
    print(f"   ✅ GET JSON: {response}")
    
    # Cleanup
    client.send_command("DEL user:profile")
    
    return True

def test_stats(client: CrabCacheClient) -> bool:
    """Testa o comando STATS"""
    print("\n📊 Testando STATS...")
    
    # Adicionar algumas chaves primeiro
    client.send_command("PUT stats_key1 value1")
    client.send_command("PUT stats_key2 value2")
    client.send_command("PUT stats_key3 value3")
    
    # STATS
    response = client.send_command("STATS")
    
    # Verificar se a resposta contém informações esperadas
    if not response.startswith("STATS:"):
        print(f"   ❌ STATS formato inválido: {response}")
        return False
    
    if "shard_" not in response:
        print(f"   ❌ STATS não contém informações de shard: {response}")
        return False
    
    if "keys" not in response:
        print(f"   ❌ STATS não contém informações de chaves: {response}")
        return False
    
    print(f"   ✅ STATS: {response}")
    
    # Cleanup
    client.send_command("DEL stats_key1")
    client.send_command("DEL stats_key2")
    client.send_command("DEL stats_key3")
    
    return True

def test_error_cases(client: CrabCacheClient) -> bool:
    """Testa casos de erro"""
    print("\n❌ Testando casos de erro...")
    
    # Comando inválido
    print("   Comando inválido")
    response = client.send_command("INVALID_COMMAND")
    if not response.startswith("ERROR:"):
        print(f"   ❌ Comando inválido deveria retornar erro: {response}")
        return False
    print(f"   ✅ Comando inválido: {response}")
    
    # GET de chave inexistente
    print("   GET de chave inexistente")
    response = client.send_command("GET nonexistent_key")
    if response != "NULL":
        print(f"   ❌ GET inexistente deveria retornar NULL: {response}")
        return False
    print(f"   ✅ GET inexistente: {response}")
    
    # DEL de chave inexistente
    print("   DEL de chave inexistente")
    response = client.send_command("DEL nonexistent_key")
    if response != "NULL":
        print(f"   ❌ DEL inexistente deveria retornar NULL: {response}")
        return False
    print(f"   ✅ DEL inexistente: {response}")
    
    # EXPIRE de chave inexistente
    print("   EXPIRE de chave inexistente")
    response = client.send_command("EXPIRE nonexistent_key 3600")
    if response != "NULL":
        print(f"   ❌ EXPIRE inexistente deveria retornar NULL: {response}")
        return False
    print(f"   ✅ EXPIRE inexistente: {response}")
    
    return True

def main():
    """Função principal"""
    print("🦀 CrabCache API Test Suite")
    print("=" * 50)
    
    # Configuração
    host = sys.argv[1] if len(sys.argv) > 1 else 'localhost'
    port = int(sys.argv[2]) if len(sys.argv) > 2 else 7000
    
    print(f"Conectando em {host}:{port}")
    
    client = CrabCacheClient(host, port)
    
    # Verificar conectividade
    try:
        response = client.send_command("PING")
        if "ERROR:" in response:
            print(f"❌ Não foi possível conectar: {response}")
            print("\n💡 Certifique-se de que o CrabCache está rodando:")
            print("   Local: cargo run")
            print("   Docker: docker run -p 7000:7000 crabcache:latest")
            return False
    except Exception as e:
        print(f"❌ Erro de conexão: {e}")
        return False
    
    # Executar testes
    tests = [
        ("PING", test_ping),
        ("PUT/GET/DEL", test_put_get_del),
        ("PUT com TTL", test_put_with_ttl),
        ("EXPIRE", test_expire_command),
        ("Dados JSON", test_json_data),
        ("STATS", test_stats),
        ("Casos de erro", test_error_cases),
    ]
    
    passed = 0
    total = len(tests)
    
    for test_name, test_func in tests:
        try:
            if test_func(client):
                passed += 1
            else:
                print(f"❌ Teste '{test_name}' falhou!")
        except Exception as e:
            print(f"❌ Erro no teste '{test_name}': {e}")
    
    # Resultado final
    print("\n" + "=" * 50)
    print(f"📊 Resultado Final: {passed}/{total} testes passaram")
    
    if passed == total:
        print("🎉 Todos os testes passaram! CrabCache está funcionando perfeitamente!")
        return True
    else:
        print(f"⚠️  {total - passed} teste(s) falharam. Verifique os logs acima.")
        return False

if __name__ == "__main__":
    success = main()
    sys.exit(0 if success else 1)