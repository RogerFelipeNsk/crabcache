#!/usr/bin/env python3
"""
Teste focado nas operações que mais se beneficiam do protocolo binário
"""

import socket
import time
import struct
from concurrent.futures import ThreadPoolExecutor

class BinaryFocusedTester:
    def __init__(self, host='localhost', port=7001):
        self.host = host
        self.port = port
    
    def create_connection(self):
        sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        sock.settimeout(10)
        sock.connect((self.host, self.port))
        return sock
    
    def test_ping_performance(self, protocol='text', operations=1000):
        """Testa performance de PING (maior benefício do protocolo binário)"""
        print(f"🏓 Testando PING {protocol.upper()} ({operations} ops)...")
        
        sock = self.create_connection()
        results = []
        
        for i in range(operations):
            start = time.perf_counter()
            
            if protocol == 'binary':
                # PING binário: 1 byte
                sock.send(bytes([0x01]))
                response = sock.recv(1024)
                # Resposta esperada: 1 byte (0x11)
                success = len(response) == 1 and response[0] == 0x11
            else:
                # PING texto: 5 bytes
                sock.send(b'PING\n')
                response = sock.recv(1024)
                # Resposta esperada: "PONG\r\n" (6 bytes)
                success = response.strip() == b'PONG'
            
            duration = time.perf_counter() - start
            results.append((success, duration, len(response)))
        
        sock.close()
        
        successful = [r for r in results if r[0]]
        if successful:
            avg_latency = sum(r[1] for r in successful) / len(successful)
            avg_response_size = sum(r[2] for r in successful) / len(successful)
            throughput = len(successful) / sum(r[1] for r in successful)
        else:
            avg_latency = 0
            avg_response_size = 0
            throughput = 0
        
        return {
            'protocol': protocol,
            'operation': 'PING',
            'total_ops': len(results),
            'successful_ops': len(successful),
            'success_rate': len(successful) / len(results) * 100,
            'avg_latency_ms': avg_latency * 1000,
            'avg_response_size': avg_response_size,
            'throughput': throughput
        }
    
    def test_simple_operations(self, protocol='text', operations=500):
        """Testa operações simples (PUT/GET pequenos, DEL)"""
        print(f"🔧 Testando operações simples {protocol.upper()} ({operations} ops)...")
        
        sock = self.create_connection()
        results = []
        
        for i in range(operations):
            # PUT pequeno
            key = f"k{i}".encode()
            value = f"v{i}".encode()
            
            start = time.perf_counter()
            
            if protocol == 'binary':
                # PUT binário
                cmd = bytearray([0x02])  # CMD_PUT
                cmd.extend(struct.pack('<I', len(key)))
                cmd.extend(key)
                cmd.extend(struct.pack('<I', len(value)))
                cmd.extend(value)
                cmd.append(0)  # no TTL
                sock.send(cmd)
                response = sock.recv(1024)
                success = len(response) == 1 and response[0] == 0x10  # RESP_OK
            else:
                # PUT texto
                cmd = f"PUT {key.decode()} {value.decode()}\n".encode()
                sock.send(cmd)
                response = sock.recv(1024)
                success = response.strip() == b'OK'
            
            duration = time.perf_counter() - start
            results.append((success, duration, len(response)))
        
        sock.close()
        
        successful = [r for r in results if r[0]]
        if successful:
            avg_latency = sum(r[1] for r in successful) / len(successful)
            avg_response_size = sum(r[2] for r in successful) / len(successful)
            throughput = len(successful) / sum(r[1] for r in successful)
        else:
            avg_latency = 0
            avg_response_size = 0
            throughput = 0
        
        return {
            'protocol': protocol,
            'operation': 'PUT_SMALL',
            'total_ops': len(results),
            'successful_ops': len(successful),
            'success_rate': len(successful) / len(results) * 100,
            'avg_latency_ms': avg_latency * 1000,
            'avg_response_size': avg_response_size,
            'throughput': throughput
        }
    
    def run_focused_comparison(self):
        """Executa comparação focada"""
        print("🎯 Teste Focado: Protocolo Binário vs Texto")
        print("=" * 60)
        
        # Teste PING (máximo benefício esperado)
        ping_text = self.test_ping_performance('text', 2000)
        ping_binary = self.test_ping_performance('binary', 2000)
        
        # Teste operações simples
        simple_text = self.test_simple_operations('text', 1000)
        simple_binary = self.test_simple_operations('binary', 1000)
        
        # Análise
        print("\n" + "=" * 70)
        print("📊 RESULTADOS FOCADOS")
        print("=" * 70)
        
        print(f"\n🏓 PING Performance:")
        print(f"   Texto:   {ping_text['throughput']:.0f} ops/sec, {ping_text['avg_latency_ms']:.2f}ms, {ping_text['avg_response_size']:.1f} bytes")
        print(f"   Binário: {ping_binary['throughput']:.0f} ops/sec, {ping_binary['avg_latency_ms']:.2f}ms, {ping_binary['avg_response_size']:.1f} bytes")
        
        if ping_text['throughput'] > 0:
            ping_improvement = ping_binary['throughput'] / ping_text['throughput']
            ping_size_reduction = (ping_text['avg_response_size'] - ping_binary['avg_response_size']) / ping_text['avg_response_size'] * 100
            print(f"   Melhoria: {ping_improvement:.2f}x throughput, {ping_size_reduction:.1f}% redução tamanho")
        
        print(f"\n🔧 PUT Pequeno Performance:")
        print(f"   Texto:   {simple_text['throughput']:.0f} ops/sec, {simple_text['avg_latency_ms']:.2f}ms, {simple_text['avg_response_size']:.1f} bytes")
        print(f"   Binário: {simple_binary['throughput']:.0f} ops/sec, {simple_binary['avg_latency_ms']:.2f}ms, {simple_binary['avg_response_size']:.1f} bytes")
        
        if simple_text['throughput'] > 0:
            simple_improvement = simple_binary['throughput'] / simple_text['throughput']
            simple_size_reduction = (simple_text['avg_response_size'] - simple_binary['avg_response_size']) / simple_text['avg_response_size'] * 100
            print(f"   Melhoria: {simple_improvement:.2f}x throughput, {simple_size_reduction:.1f}% redução tamanho")
        
        print("\n" + "=" * 70)
        
        return {
            'ping': {'text': ping_text, 'binary': ping_binary},
            'simple': {'text': simple_text, 'binary': simple_binary}
        }

def main():
    tester = BinaryFocusedTester()
    
    # Teste de conectividade
    try:
        sock = tester.create_connection()
        sock.close()
        print("✅ Conectividade OK")
    except Exception as e:
        print(f"❌ Erro de conectividade: {e}")
        return 1
    
    # Executar teste focado
    results = tester.run_focused_comparison()
    
    return 0

if __name__ == '__main__':
    exit(main())