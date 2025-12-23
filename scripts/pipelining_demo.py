#!/usr/bin/env python3
"""
Pipelining Demo

This script demonstrates the difference between pipelined and non-pipelined
operations with visual timing comparisons.
"""

import socket
import struct
import time
import threading
from typing import List, Dict

# Binary protocol constants
CMD_PING = 0x01
RESP_PONG = 0x11

class PipeliningDemo:
    """Demonstrate pipelining vs non-pipelining performance"""
    
    def __init__(self, host="127.0.0.1", port=7001):
        self.host = host
        self.port = port
    
    def connect(self) -> socket.socket:
        """Create connection to CrabCache"""
        sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        sock.setsockopt(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)
        sock.connect((self.host, self.port))
        return sock
    
    def demo_without_pipelining(self, commands_count: int = 10):
        """Demonstrate traditional request-response pattern"""
        print("🔄 SEM Pipelining (Request-Response Tradicional):")
        print("=" * 55)
        
        sock = self.connect()
        ping_cmd = bytes([CMD_PING])
        
        print("Enviando comandos um por vez...")
        print()
        
        start_time = time.perf_counter()
        latencies = []
        
        for i in range(commands_count):
            # Enviar comando individual
            cmd_start = time.perf_counter_ns()
            print(f"  {i+1:2d}. Cliente → Servidor: PING")
            
            sock.send(ping_cmd)
            response = sock.recv(1)
            
            cmd_end = time.perf_counter_ns()
            latency_ms = (cmd_end - cmd_start) / 1_000_000
            latencies.append(latency_ms)
            
            if len(response) == 1 and response[0] == RESP_PONG:
                print(f"      Cliente ← Servidor: PONG ({latency_ms:.3f}ms)")
            else:
                print(f"      Cliente ← Servidor: ERROR")
            
            print()
        
        end_time = time.perf_counter()
        total_time = end_time - start_time
        
        sock.close()
        
        print("📊 Resultados SEM Pipelining:")
        print(f"  Total de comandos: {commands_count}")
        print(f"  Tempo total: {total_time:.3f}s")
        print(f"  Throughput: {commands_count / total_time:.0f} ops/sec")
        print(f"  Latência média: {sum(latencies) / len(latencies):.3f}ms")
        print(f"  Round trips: {commands_count} (1 por comando)")
        print()
        
        return {
            "commands": commands_count,
            "total_time": total_time,
            "throughput": commands_count / total_time,
            "avg_latency": sum(latencies) / len(latencies),
            "round_trips": commands_count
        }
    
    def demo_with_pipelining(self, commands_count: int = 10, batch_size: int = 5):
        """Demonstrate pipelining pattern (simulated)"""
        print("⚡ COM Pipelining (Lote de Comandos):")
        print("=" * 45)
        
        sock = self.connect()
        ping_cmd = bytes([CMD_PING])
        
        print(f"Enviando comandos em lotes de {batch_size}...")
        print()
        
        start_time = time.perf_counter()
        total_latencies = []
        round_trips = 0
        
        # Processar em lotes
        for batch_start in range(0, commands_count, batch_size):
            batch_end = min(batch_start + batch_size, commands_count)
            current_batch_size = batch_end - batch_start
            
            print(f"📦 Lote {batch_start // batch_size + 1}:")
            
            # Enviar lote de comandos
            batch_start_time = time.perf_counter_ns()
            
            print(f"  Cliente → Servidor: {current_batch_size} comandos PING")
            for i in range(current_batch_size):
                sock.send(ping_cmd)
            
            # Receber lote de respostas
            print(f"  Cliente ← Servidor: {current_batch_size} respostas PONG")
            batch_latencies = []
            
            for i in range(current_batch_size):
                response = sock.recv(1)
                if len(response) == 1 and response[0] == RESP_PONG:
                    pass  # Resposta válida
                else:
                    print(f"    Erro na resposta {i+1}")
            
            batch_end_time = time.perf_counter_ns()
            batch_latency_ms = (batch_end_time - batch_start_time) / 1_000_000
            avg_cmd_latency = batch_latency_ms / current_batch_size
            
            print(f"  Tempo do lote: {batch_latency_ms:.3f}ms")
            print(f"  Latência por comando: {avg_cmd_latency:.3f}ms")
            print()
            
            total_latencies.extend([avg_cmd_latency] * current_batch_size)
            round_trips += 1
        
        end_time = time.perf_counter()
        total_time = end_time - start_time
        
        sock.close()
        
        print("📊 Resultados COM Pipelining:")
        print(f"  Total de comandos: {commands_count}")
        print(f"  Tamanho do lote: {batch_size}")
        print(f"  Tempo total: {total_time:.3f}s")
        print(f"  Throughput: {commands_count / total_time:.0f} ops/sec")
        print(f"  Latência média: {sum(total_latencies) / len(total_latencies):.3f}ms")
        print(f"  Round trips: {round_trips} (vs {commands_count} sem pipeline)")
        print()
        
        return {
            "commands": commands_count,
            "batch_size": batch_size,
            "total_time": total_time,
            "throughput": commands_count / total_time,
            "avg_latency": sum(total_latencies) / len(total_latencies),
            "round_trips": round_trips
        }
    
    def compare_results(self, no_pipeline: Dict, with_pipeline: Dict):
        """Compare pipelining vs non-pipelining results"""
        print("🥊 Comparação: Pipelining vs Sem Pipelining")
        print("=" * 50)
        
        throughput_improvement = with_pipeline["throughput"] / no_pipeline["throughput"]
        latency_improvement = no_pipeline["avg_latency"] / with_pipeline["avg_latency"]
        round_trip_reduction = no_pipeline["round_trips"] / with_pipeline["round_trips"]
        
        print("📊 Métricas de Performance:")
        print(f"{'Métrica':<20} {'Sem Pipeline':<15} {'Com Pipeline':<15} {'Melhoria'}")
        print("-" * 70)
        print(f"{'Throughput':<20} {no_pipeline['throughput']:<14.0f} {with_pipeline['throughput']:<14.0f} {throughput_improvement:.1f}x")
        print(f"{'Latência Média':<20} {no_pipeline['avg_latency']:<13.3f}ms {with_pipeline['avg_latency']:<13.3f}ms {latency_improvement:.1f}x")
        print(f"{'Round Trips':<20} {no_pipeline['round_trips']:<14} {with_pipeline['round_trips']:<14} {round_trip_reduction:.1f}x")
        print()
        
        print("💡 Análise:")
        if throughput_improvement > 2:
            print(f"  🚀 EXCELENTE! Pipelining melhorou throughput em {throughput_improvement:.1f}x")
        elif throughput_improvement > 1.5:
            print(f"  ✅ BOM! Pipelining melhorou throughput em {throughput_improvement:.1f}x")
        else:
            print(f"  ⚠️  Melhoria modesta: {throughput_improvement:.1f}x")
        
        if latency_improvement > 2:
            print(f"  ⚡ Latência reduzida em {latency_improvement:.1f}x (menos tempo por comando)")
        
        print(f"  📡 Reduziu round trips de {no_pipeline['round_trips']} para {with_pipeline['round_trips']}")
        
        print()
        print("🎯 Projeção para CrabCache:")
        current_throughput = 19634  # Resultado do teste anterior
        projected_throughput = current_throughput * throughput_improvement
        
        print(f"  Performance atual: {current_throughput:,} ops/sec")
        print(f"  Com pipelining: {projected_throughput:,.0f} ops/sec")
        print(f"  vs Redis (37,498): {projected_throughput / 37498:.1f}x")
        
        if projected_throughput > 37498:
            surplus = projected_throughput - 37498
            print(f"  🏆 SUPERARIA Redis em {surplus:,.0f} ops/sec!")
        
    def run_demo(self):
        """Run complete pipelining demonstration"""
        print("🚀 Demonstração de Pipelining no CrabCache")
        print("=" * 50)
        print("🎯 OBJETIVO: Mostrar a diferença entre pipelining e sem pipelining")
        print()
        
        commands_count = 20
        batch_size = 5
        
        print(f"📋 Configuração do Teste:")
        print(f"  Comandos totais: {commands_count}")
        print(f"  Tamanho do lote: {batch_size}")
        print(f"  Servidor: {self.host}:{self.port}")
        print()
        
        try:
            # Teste sem pipelining
            no_pipeline_results = self.demo_without_pipelining(commands_count)
            
            print("\n" + "="*60 + "\n")
            
            # Teste com pipelining (simulado)
            pipeline_results = self.demo_with_pipelining(commands_count, batch_size)
            
            print("\n" + "="*60 + "\n")
            
            # Comparação
            self.compare_results(no_pipeline_results, pipeline_results)
            
        except Exception as e:
            print(f"❌ Erro na demonstração: {e}")
            print("💡 Certifique-se de que o CrabCache está rodando na porta 7001")

def main():
    print("🔍 Iniciando Demonstração de Pipelining...")
    print()
    
    demo = PipeliningDemo()
    demo.run_demo()
    
    print("\n🎊 Demonstração Completa!")
    print()
    print("📚 Conceitos Importantes:")
    print("  • Pipelining = enviar múltiplos comandos juntos")
    print("  • Reduz round trips de rede")
    print("  • Melhora throughput significativamente")
    print("  • Redis usa pipelining por padrão no benchmark")
    print("  • CrabCache pode superar Redis com pipelining!")

if __name__ == "__main__":
    main()