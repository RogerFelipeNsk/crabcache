#!/usr/bin/env python3
"""
Teste do protocolo binário do CrabCache
Compara performance entre protocolo texto e binário
"""

import socket
import time
import struct
import threading
from concurrent.futures import ThreadPoolExecutor
from dataclasses import dataclass
from typing import List, Dict, Any

# Constantes do protocolo binário
CMD_PING = 0x01
CMD_PUT = 0x02
CMD_GET = 0x03
CMD_DEL = 0x04
CMD_STATS = 0x06

RESP_OK = 0x10
RESP_PONG = 0x11
RESP_NULL = 0x12
RESP_ERROR = 0x13
RESP_VALUE = 0x14
RESP_STATS = 0x15

@dataclass
class TestResult:
    protocol: str
    operation: str
    success: bool
    duration: float
    response_size: int

class BinaryProtocolTester:
    def __init__(self, host='localhost', port=7001):
        self.host = host
        self.port = port
    
    def create_connection(self) -> socket.socket:
        """Cria conexão TCP"""
        sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        sock.settimeout(10)
        sock.connect((self.host, self.port))
        return sock
    
    # Protocolo Binário
    def serialize_binary_command(self, command: str, key: bytes = b"", value: bytes = b"") -> bytes:
        """Serializa comando para protocolo binário"""
        if command == "PING":
            return bytes([CMD_PING])
        elif command == "PUT":
            data = bytearray()
            data.append(CMD_PUT)
            data.extend(struct.pack('<I', len(key)))  # key length (little-endian)
            data.extend(key)
            data.extend(struct.pack('<I', len(value)))  # value length
            data.extend(value)
            data.append(0)  # no TTL
            return bytes(data)
        elif command == "GET":
            data = bytearray()
            data.append(CMD_GET)
            data.extend(struct.pack('<I', len(key)))
            data.extend(key)
            return bytes(data)
        elif command == "DEL":
            data = bytearray()
            data.append(CMD_DEL)
            data.extend(struct.pack('<I', len(key)))
            data.extend(key)
            return bytes(data)
        elif command == "STATS":
            return bytes([CMD_STATS])
        else:
            raise ValueError(f"Unknown command: {command}")
    
    def parse_binary_response(self, data: bytes) -> tuple[bool, str, int]:
        """Parseia resposta do protocolo binário"""
        if not data:
            return False, "Empty response", 0
        
        response_type = data[0]
        
        if response_type == RESP_OK:
            return True, "OK", len(data)
        elif response_type == RESP_PONG:
            return True, "PONG", len(data)
        elif response_type == RESP_NULL:
            return True, "NULL", len(data)
        elif response_type == RESP_VALUE:
            if len(data) < 5:
                return False, "Invalid VALUE response", len(data)
            value_len = struct.unpack('<I', data[1:5])[0]
            if len(data) < 5 + value_len:
                return False, "Incomplete VALUE response", len(data)
            value = data[5:5+value_len].decode('utf-8', errors='ignore')
            return True, value, len(data)
        elif response_type == RESP_STATS:
            if len(data) < 5:
                return False, "Invalid STATS response", len(data)
            stats_len = struct.unpack('<I', data[1:5])[0]
            if len(data) < 5 + stats_len:
                return False, "Incomplete STATS response", len(data)
            stats = data[5:5+stats_len].decode('utf-8', errors='ignore')
            return True, stats, len(data)
        elif response_type == RESP_ERROR:
            if len(data) < 5:
                return False, "Invalid ERROR response", len(data)
            error_len = struct.unpack('<I', data[1:5])[0]
            if len(data) < 5 + error_len:
                return False, "Incomplete ERROR response", len(data)
            error = data[5:5+error_len].decode('utf-8', errors='ignore')
            return False, f"ERROR: {error}", len(data)
        else:
            return False, f"Unknown response type: {response_type}", len(data)
    
    # Protocolo Texto
    def serialize_text_command(self, command: str, key: str = "", value: str = "") -> bytes:
        """Serializa comando para protocolo texto"""
        if command == "PING":
            return b"PING\n"
        elif command == "PUT":
            return f"PUT {key} {value}\n".encode()
        elif command == "GET":
            return f"GET {key}\n".encode()
        elif command == "DEL":
            return f"DEL {key}\n".encode()
        elif command == "STATS":
            return b"STATS\n"
        else:
            raise ValueError(f"Unknown command: {command}")
    
    def parse_text_response(self, data: bytes) -> tuple[bool, str, int]:
        """Parseia resposta do protocolo texto"""
        try:
            response = data.decode().strip()
            if response == "OK":
                return True, "OK", len(data)
            elif response == "PONG":
                return True, "PONG", len(data)
            elif response == "NULL":
                return True, "NULL", len(data)
            elif response.startswith("ERROR"):
                return False, response, len(data)
            elif response.startswith("STATS:"):
                return True, response, len(data)
            else:
                # Assume it's a value
                return True, response, len(data)
        except:
            return False, "Parse error", len(data)
    
    def test_single_operation(self, protocol: str, command: str, key: str = "", value: str = "") -> TestResult:
        """Testa uma operação específica"""
        start_time = time.perf_counter()
        
        try:
            sock = self.create_connection()
            
            # Serializar comando
            if protocol == "binary":
                cmd_data = self.serialize_binary_command(command, key.encode(), value.encode())
            else:
                cmd_data = self.serialize_text_command(command, key, value)
            
            # Enviar comando
            sock.send(cmd_data)
            
            # Receber resposta
            response_data = sock.recv(4096)
            
            # Parsear resposta
            if protocol == "binary":
                success, response, response_size = self.parse_binary_response(response_data)
            else:
                success, response, response_size = self.parse_text_response(response_data)
            
            duration = time.perf_counter() - start_time
            sock.close()
            
            return TestResult(
                protocol=protocol,
                operation=command,
                success=success,
                duration=duration,
                response_size=response_size
            )
            
        except Exception as e:
            duration = time.perf_counter() - start_time
            return TestResult(
                protocol=protocol,
                operation=command,
                success=False,
                duration=duration,
                response_size=0
            )
    
    def benchmark_protocol(self, protocol: str, operations: int = 1000) -> Dict[str, Any]:
        """Benchmark de um protocolo específico"""
        print(f"🚀 Benchmarking {protocol.upper()} protocol ({operations} operations)...")
        
        results = []
        start_time = time.perf_counter()
        
        # Mix de operações realistas
        for i in range(operations):
            if i % 100 == 0:
                # PING a cada 100 operações
                result = self.test_single_operation(protocol, "PING")
                results.append(result)
            elif i % 50 == 0:
                # STATS a cada 50 operações
                result = self.test_single_operation(protocol, "STATS")
                results.append(result)
            elif i % 4 == 0:
                # PUT a cada 4 operações
                key = f"bench_key_{i}"
                value = f"bench_value_{i}_{'x' * 20}"  # ~35 bytes
                result = self.test_single_operation(protocol, "PUT", key, value)
                results.append(result)
            elif i % 4 == 1:
                # GET
                key = f"bench_key_{i-1}"  # Get previous key
                result = self.test_single_operation(protocol, "GET", key)
                results.append(result)
            elif i % 4 == 2:
                # Another GET
                key = f"bench_key_{i-2}"
                result = self.test_single_operation(protocol, "GET", key)
                results.append(result)
            else:
                # DEL
                key = f"bench_key_{i-3}"
                result = self.test_single_operation(protocol, "DEL", key)
                results.append(result)
        
        total_time = time.perf_counter() - start_time
        
        # Análise dos resultados
        successful_ops = [r for r in results if r.success]
        failed_ops = [r for r in results if not r.success]
        
        if successful_ops:
            avg_duration = sum(r.duration for r in successful_ops) / len(successful_ops)
            avg_response_size = sum(r.response_size for r in successful_ops) / len(successful_ops)
            durations = sorted([r.duration for r in successful_ops])
            p95_duration = durations[int(len(durations) * 0.95)]
        else:
            avg_duration = 0
            avg_response_size = 0
            p95_duration = 0
        
        return {
            'protocol': protocol,
            'total_operations': len(results),
            'successful_operations': len(successful_ops),
            'failed_operations': len(failed_ops),
            'success_rate': len(successful_ops) / len(results) * 100 if results else 0,
            'total_time': total_time,
            'throughput': len(results) / total_time,
            'avg_latency_ms': avg_duration * 1000,
            'p95_latency_ms': p95_duration * 1000,
            'avg_response_size': avg_response_size,
            'results': results
        }
    
    def compare_protocols(self, operations: int = 1000) -> Dict[str, Any]:
        """Compara protocolos binário e texto"""
        print("🥊 Comparando Protocolos: Binário vs Texto")
        print("=" * 50)
        
        # Benchmark protocolo texto
        text_results = self.benchmark_protocol("text", operations)
        
        # Benchmark protocolo binário
        binary_results = self.benchmark_protocol("binary", operations)
        
        # Comparação
        comparison = {
            'text': text_results,
            'binary': binary_results,
            'improvements': {}
        }
        
        if text_results['throughput'] > 0:
            comparison['improvements'] = {
                'throughput_improvement': binary_results['throughput'] / text_results['throughput'],
                'latency_improvement': text_results['avg_latency_ms'] / binary_results['avg_latency_ms'] if binary_results['avg_latency_ms'] > 0 else float('inf'),
                'response_size_reduction': (text_results['avg_response_size'] - binary_results['avg_response_size']) / text_results['avg_response_size'] * 100 if text_results['avg_response_size'] > 0 else 0,
            }
        
        return comparison
    
    def print_comparison(self, comparison: Dict[str, Any]):
        """Imprime comparação formatada"""
        text = comparison['text']
        binary = comparison['binary']
        improvements = comparison['improvements']
        
        print("\n" + "=" * 70)
        print("📊 COMPARAÇÃO: Protocolo Binário vs Texto")
        print("=" * 70)
        
        print(f"📈 Performance:")
        print(f"   📝 Texto:    {text['throughput']:.0f} ops/sec, {text['avg_latency_ms']:.2f}ms latência")
        print(f"   🔢 Binário:  {binary['throughput']:.0f} ops/sec, {binary['avg_latency_ms']:.2f}ms latência")
        
        if improvements:
            print(f"\n🚀 Melhorias:")
            print(f"   Throughput: {improvements['throughput_improvement']:.2f}x mais rápido")
            print(f"   Latência:   {improvements['latency_improvement']:.2f}x menor")
            print(f"   Tamanho:    {improvements['response_size_reduction']:.1f}% redução")
        
        print(f"\n📊 Detalhes:")
        print(f"   Texto:   {text['successful_operations']}/{text['total_operations']} sucessos ({text['success_rate']:.1f}%)")
        print(f"   Binário: {binary['successful_operations']}/{binary['total_operations']} sucessos ({binary['success_rate']:.1f}%)")
        
        print(f"\n📦 Tamanho Médio de Resposta:")
        print(f"   Texto:   {text['avg_response_size']:.1f} bytes")
        print(f"   Binário: {binary['avg_response_size']:.1f} bytes")
        
        print("\n" + "=" * 70)

def main():
    import argparse
    
    parser = argparse.ArgumentParser(description='Teste do protocolo binário CrabCache')
    parser.add_argument('--host', default='localhost', help='Host do CrabCache')
    parser.add_argument('--port', type=int, default=7001, help='Porta do CrabCache')
    parser.add_argument('--operations', type=int, default=1000, help='Número de operações por protocolo')
    
    args = parser.parse_args()
    
    tester = BinaryProtocolTester(args.host, args.port)
    
    # Teste de conectividade
    try:
        sock = tester.create_connection()
        sock.close()
        print("✅ Conectividade OK")
    except Exception as e:
        print(f"❌ Erro de conectividade: {e}")
        return 1
    
    # Comparar protocolos
    comparison = tester.compare_protocols(args.operations)
    tester.print_comparison(comparison)
    
    return 0

if __name__ == '__main__':
    exit(main())