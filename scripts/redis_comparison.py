#!/usr/bin/env python3
"""
Comparação de Performance: CrabCache vs Redis
Executa testes paralelos e compara resultados
"""

import subprocess
import json
import time
import os
import sys
from typing import Dict, Any

def run_redis_benchmark(host: str = 'localhost', port: int = 6379, 
                       clients: int = 10, requests: int = 10000) -> Dict[str, Any]:
    """Executa benchmark do Redis usando redis-benchmark"""
    try:
        # Verificar se redis-benchmark está disponível
        subprocess.run(['redis-benchmark', '--version'], 
                      capture_output=True, check=True)
    except (subprocess.CalledProcessError, FileNotFoundError):
        return {"error": "redis-benchmark não encontrado. Instale Redis tools."}
    
    try:
        # Executar redis-benchmark
        cmd = [
            'redis-benchmark',
            '-h', host,
            '-p', str(port),
            '-c', str(clients),
            '-n', str(requests),
            '-t', 'set,get,del,ping',
            '--csv'
        ]
        
        result = subprocess.run(cmd, capture_output=True, text=True, timeout=300)
        
        if result.returncode != 0:
            return {"error": f"redis-benchmark falhou: {result.stderr}"}
        
        # Parsear resultados CSV
        lines = result.stdout.strip().split('\n')
        redis_results = {}
        
        for line in lines:
            if ',' in line:
                parts = line.split(',')
                if len(parts) >= 2:
                    test_name = parts[0].strip('"')
                    ops_per_sec = float(parts[1].strip('"'))
                    redis_results[test_name] = {
                        'ops_per_second': ops_per_sec,
                        'clients': clients,
                        'requests': requests
                    }
        
        return {
            'redis_results': redis_results,
            'total_requests': requests,
            'clients': clients
        }
        
    except subprocess.TimeoutExpired:
        return {"error": "Redis benchmark timeout"}
    except Exception as e:
        return {"error": f"Erro no Redis benchmark: {e}"}

def run_crabcache_benchmark(host: str = 'localhost', port: int = 7001,
                           clients: int = 10, duration: int = 30) -> Dict[str, Any]:
    """Executa benchmark do CrabCache"""
    script_dir = os.path.dirname(os.path.abspath(__file__))
    tcp_test_script = os.path.join(script_dir, 'tcp_load_test.py')
    
    if not os.path.exists(tcp_test_script):
        return {"error": "Script tcp_load_test.py não encontrado"}
    
    try:
        # Calcular ops_per_sec para ter número similar de operações
        target_ops = 10000  # Similar ao Redis benchmark
        ops_per_sec = max(50, target_ops // (duration * clients))
        
        cmd = [
            'python3', tcp_test_script,
            '--host', host,
            '--port', str(port),
            '--users', str(clients),
            '--duration', str(duration),
            '--ops-per-sec', str(ops_per_sec)
        ]
        
        result = subprocess.run(cmd, capture_output=True, text=True, timeout=300)
        
        if result.returncode != 0:
            return {"error": f"CrabCache benchmark falhou: {result.stderr}"}
        
        # Extrair JSON do output (última linha que contém JSON)
        lines = result.stdout.strip().split('\n')
        json_output = None
        
        for line in reversed(lines):
            if line.strip().startswith('{'):
                try:
                    json_output = json.loads(line)
                    break
                except:
                    continue
        
        if not json_output:
            # Se não encontrou JSON no stdout, tentar parsear manualmente
            return {"error": "Não foi possível extrair resultados JSON"}
        
        return json_output
        
    except subprocess.TimeoutExpired:
        return {"error": "CrabCache benchmark timeout"}
    except Exception as e:
        return {"error": f"Erro no CrabCache benchmark: {e}"}

def compare_results(crabcache_results: Dict[str, Any], 
                   redis_results: Dict[str, Any]) -> Dict[str, Any]:
    """Compara resultados dos dois sistemas"""
    
    if 'error' in crabcache_results or 'error' in redis_results:
        return {
            "error": "Erro em um dos benchmarks",
            "crabcache_error": crabcache_results.get('error'),
            "redis_error": redis_results.get('error')
        }
    
    # Mapear operações do CrabCache para Redis
    operation_mapping = {
        'PUT': 'SET',
        'GET': 'GET', 
        'DEL': 'DEL',
        'PING': 'PING'
    }
    
    comparison = {
        'summary': {
            'crabcache_total_ops': crabcache_results['global_metrics']['total_operations'],
            'crabcache_throughput': crabcache_results['global_metrics']['actual_ops_per_second'],
            'crabcache_success_rate': crabcache_results['global_metrics']['success_rate'],
            'crabcache_p95_latency': crabcache_results['global_metrics']['p95_latency_ms'],
            'redis_total_requests': redis_results.get('total_requests', 0),
            'redis_clients': redis_results.get('clients', 0)
        },
        'operation_comparison': {},
        'winner': {}
    }
    
    redis_ops = redis_results.get('redis_results', {})
    crabcache_ops = crabcache_results.get('operations', {})
    
    # Comparar operações
    for crab_op, redis_op in operation_mapping.items():
        if crab_op in crabcache_ops and redis_op in redis_ops:
            crab_stats = crabcache_ops[crab_op]
            redis_stats = redis_ops[redis_op]
            
            crab_throughput = crab_stats['ops_per_second']
            redis_throughput = redis_stats['ops_per_second']
            
            comparison['operation_comparison'][crab_op] = {
                'crabcache': {
                    'ops_per_second': crab_throughput,
                    'success_rate': crab_stats['success_rate'],
                    'p95_latency_ms': crab_stats['p95_latency_ms']
                },
                'redis': {
                    'ops_per_second': redis_throughput
                },
                'performance_ratio': crab_throughput / redis_throughput if redis_throughput > 0 else 0,
                'winner': 'CrabCache' if crab_throughput > redis_throughput else 'Redis'
            }
    
    # Determinar vencedor geral
    total_crab_throughput = comparison['summary']['crabcache_throughput']
    
    # Calcular throughput médio do Redis
    redis_throughputs = [stats['ops_per_second'] for stats in redis_ops.values()]
    avg_redis_throughput = sum(redis_throughputs) / len(redis_throughputs) if redis_throughputs else 0
    
    comparison['summary']['redis_avg_throughput'] = avg_redis_throughput
    comparison['summary']['overall_winner'] = 'CrabCache' if total_crab_throughput > avg_redis_throughput else 'Redis'
    comparison['summary']['performance_ratio'] = total_crab_throughput / avg_redis_throughput if avg_redis_throughput > 0 else 0
    
    return comparison

def print_comparison(comparison: Dict[str, Any]):
    """Imprime comparação formatada"""
    
    if 'error' in comparison:
        print(f"❌ Erro na comparação: {comparison['error']}")
        if comparison.get('crabcache_error'):
            print(f"   CrabCache: {comparison['crabcache_error']}")
        if comparison.get('redis_error'):
            print(f"   Redis: {comparison['redis_error']}")
        return
    
    summary = comparison['summary']
    
    print("\n" + "="*70)
    print("🥊 COMPARAÇÃO: CrabCache vs Redis")
    print("="*70)
    
    print(f"📊 Resumo Geral:")
    print(f"   🦀 CrabCache: {summary['crabcache_throughput']:.1f} ops/sec")
    print(f"   🔴 Redis:     {summary['redis_avg_throughput']:.1f} ops/sec")
    print(f"   🏆 Vencedor:  {summary['overall_winner']}")
    print(f"   📈 Ratio:     {summary['performance_ratio']:.2f}x")
    print()
    
    print(f"📋 Detalhes:")
    print(f"   CrabCache: {summary['crabcache_total_ops']:,} ops, {summary['crabcache_success_rate']:.1f}% sucesso")
    print(f"   CrabCache P95: {summary['crabcache_p95_latency']:.2f}ms")
    print(f"   Redis: {summary['redis_total_requests']:,} requests, {summary['redis_clients']} clients")
    print()
    
    print("🔍 Comparação por Operação:")
    for op, stats in comparison['operation_comparison'].items():
        crab = stats['crabcache']
        redis = stats['redis']
        ratio = stats['performance_ratio']
        winner = stats['winner']
        
        print(f"   🔸 {op}:")
        print(f"      CrabCache: {crab['ops_per_second']:.1f} ops/sec ({crab['success_rate']:.1f}% sucesso)")
        print(f"      Redis:     {redis['ops_per_second']:.1f} ops/sec")
        print(f"      Ratio:     {ratio:.2f}x | Vencedor: {winner}")
    
    print("\n" + "="*70)

def main():
    import argparse
    
    parser = argparse.ArgumentParser(description='Comparação CrabCache vs Redis')
    parser.add_argument('--crabcache-host', default='localhost', help='Host do CrabCache')
    parser.add_argument('--crabcache-port', type=int, default=7001, help='Porta do CrabCache')
    parser.add_argument('--redis-host', default='localhost', help='Host do Redis')
    parser.add_argument('--redis-port', type=int, default=6379, help='Porta do Redis')
    parser.add_argument('--clients', type=int, default=10, help='Número de clientes')
    parser.add_argument('--requests', type=int, default=10000, help='Número de requests (Redis)')
    parser.add_argument('--duration', type=int, default=30, help='Duração em segundos (CrabCache)')
    parser.add_argument('--output', help='Arquivo para salvar resultados')
    
    args = parser.parse_args()
    
    print("🚀 Iniciando Comparação: CrabCache vs Redis")
    print("=" * 50)
    
    # Executar benchmark do Redis
    print("🔴 Executando benchmark do Redis...")
    redis_results = run_redis_benchmark(
        args.redis_host, args.redis_port, 
        args.clients, args.requests
    )
    
    if 'error' in redis_results:
        print(f"❌ Erro no Redis: {redis_results['error']}")
    else:
        print("✅ Redis benchmark concluído")
    
    # Executar benchmark do CrabCache
    print("🦀 Executando benchmark do CrabCache...")
    crabcache_results = run_crabcache_benchmark(
        args.crabcache_host, args.crabcache_port,
        args.clients, args.duration
    )
    
    if 'error' in crabcache_results:
        print(f"❌ Erro no CrabCache: {crabcache_results['error']}")
    else:
        print("✅ CrabCache benchmark concluído")
    
    # Comparar resultados
    comparison = compare_results(crabcache_results, redis_results)
    
    # Exibir comparação
    print_comparison(comparison)
    
    # Salvar resultados se solicitado
    if args.output:
        full_results = {
            'comparison': comparison,
            'crabcache_results': crabcache_results,
            'redis_results': redis_results,
            'timestamp': time.time(),
            'config': {
                'clients': args.clients,
                'requests': args.requests,
                'duration': args.duration
            }
        }
        
        with open(args.output, 'w') as f:
            json.dump(full_results, f, indent=2)
        print(f"💾 Resultados salvos em: {args.output}")
    
    # Código de saída baseado no resultado
    if 'error' in comparison:
        return 1
    
    return 0

if __name__ == '__main__':
    exit(main())