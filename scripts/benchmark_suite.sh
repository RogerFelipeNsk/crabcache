#!/bin/bash

# Suite de Benchmarks para CrabCache
# Executa diferentes cenários de teste de performance

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"

# Configurações padrão
CRABCACHE_HOST="localhost"
CRABCACHE_PORT="7001"
RESULTS_DIR="$PROJECT_DIR/benchmark_results"

echo "🚀 CrabCache Benchmark Suite"
echo "============================"
echo "Host: $CRABCACHE_HOST:$CRABCACHE_PORT"
echo "Resultados: $RESULTS_DIR"
echo ""

# Criar diretório de resultados
mkdir -p "$RESULTS_DIR"

# Função para executar benchmark
run_benchmark() {
    local name="$1"
    local users="$2"
    local duration="$3"
    local ops_per_sec="$4"
    local description="$5"
    
    echo "📊 Executando: $name"
    echo "   $description"
    echo "   Usuários: $users, Duração: ${duration}s, Ops/sec: $ops_per_sec"
    
    local output_file="$RESULTS_DIR/${name}_$(date +%Y%m%d_%H%M%S).json"
    
    python3 "$SCRIPT_DIR/tcp_load_test.py" \
        --host "$CRABCACHE_HOST" \
        --port "$CRABCACHE_PORT" \
        --users "$users" \
        --duration "$duration" \
        --ops-per-sec "$ops_per_sec" \
        --output "$output_file"
    
    echo "   ✅ Concluído: $output_file"
    echo ""
}

# Verificar se CrabCache está rodando
echo "🔍 Verificando conectividade..."
if ! nc -z "$CRABCACHE_HOST" "$CRABCACHE_PORT" 2>/dev/null; then
    echo "❌ CrabCache não está acessível em $CRABCACHE_HOST:$CRABCACHE_PORT"
    echo "   Inicie o CrabCache primeiro:"
    echo "   ./scripts/docker-start.sh"
    exit 1
fi
echo "✅ CrabCache acessível"
echo ""

# Suite de benchmarks
echo "🎯 Iniciando suite de benchmarks..."
echo ""

# 1. Baseline - Carga baixa
run_benchmark "baseline_low" 5 30 50 \
    "Baseline com carga baixa para estabelecer métricas base"

# 2. Carga média - Cenário típico
run_benchmark "typical_load" 10 60 100 \
    "Carga típica de produção com 10 usuários"

# 3. Carga alta - Teste de stress
run_benchmark "high_load" 20 60 150 \
    "Carga alta para testar limites do sistema"

# 4. Burst test - Picos de tráfego
run_benchmark "burst_test" 50 30 200 \
    "Teste de burst - picos intensos de tráfego"

# 5. Sustained load - Carga sustentada
run_benchmark "sustained_load" 15 120 100 \
    "Carga sustentada por período prolongado"

# 6. Low latency test - Foco em latência
run_benchmark "low_latency" 5 60 50 \
    "Teste focado em latência mínima"

# 7. High throughput test - Foco em throughput
run_benchmark "high_throughput" 30 60 200 \
    "Teste focado em throughput máximo"

echo "🎉 Suite de benchmarks concluída!"
echo ""

# Gerar relatório consolidado
echo "📋 Gerando relatório consolidado..."

REPORT_FILE="$RESULTS_DIR/benchmark_report_$(date +%Y%m%d_%H%M%S).md"

cat > "$REPORT_FILE" << EOF
# CrabCache Benchmark Report

**Data:** $(date)
**Host:** $CRABCACHE_HOST:$CRABCACHE_PORT

## Resumo dos Testes

| Teste | Usuários | Duração | Ops/sec Alvo | Arquivo |
|-------|----------|---------|--------------|---------|
EOF

# Adicionar linha para cada teste
for test_name in "baseline_low" "typical_load" "high_load" "burst_test" "sustained_load" "low_latency" "high_throughput"; do
    latest_file=$(ls -t "$RESULTS_DIR"/${test_name}_*.json 2>/dev/null | head -1 || echo "N/A")
    if [[ "$latest_file" != "N/A" ]]; then
        filename=$(basename "$latest_file")
        
        # Extrair métricas do JSON
        total_ops=$(python3 -c "import json; data=json.load(open('$latest_file')); print(data['global_metrics']['total_operations'])" 2>/dev/null || echo "N/A")
        success_rate=$(python3 -c "import json; data=json.load(open('$latest_file')); print(f\"{data['global_metrics']['success_rate']:.1f}%\")" 2>/dev/null || echo "N/A")
        throughput=$(python3 -c "import json; data=json.load(open('$latest_file')); print(f\"{data['global_metrics']['actual_ops_per_second']:.1f}\")" 2>/dev/null || echo "N/A")
        p95_latency=$(python3 -c "import json; data=json.load(open('$latest_file')); print(f\"{data['global_metrics']['p95_latency_ms']:.2f}ms\")" 2>/dev/null || echo "N/A")
        
        echo "| $test_name | - | - | - | $filename |" >> "$REPORT_FILE"
        echo "|  | Total Ops: $total_ops | Success: $success_rate | Throughput: $throughput ops/sec | P95: $p95_latency |" >> "$REPORT_FILE"
    else
        echo "| $test_name | - | - | - | Não executado |" >> "$REPORT_FILE"
    fi
done

cat >> "$REPORT_FILE" << EOF

## Análise de Performance

### Métricas Chave
- **Throughput Máximo:** Verificar teste high_throughput
- **Latência Mínima:** Verificar teste low_latency  
- **Estabilidade:** Verificar teste sustained_load
- **Picos de Tráfego:** Verificar teste burst_test

### Recomendações
1. Analisar gargalos identificados nos testes de alta carga
2. Verificar distribuição de latência nos percentis P95/P99
3. Monitorar taxa de sucesso em cenários de stress
4. Comparar com benchmarks de sistemas similares (Redis, Dragonfly)

## Arquivos de Resultados
Todos os resultados detalhados estão disponíveis em:
\`$RESULTS_DIR\`

Para analisar um resultado específico:
\`\`\`bash
python3 -m json.tool $RESULTS_DIR/[arquivo].json
\`\`\`
EOF

echo "📄 Relatório gerado: $REPORT_FILE"
echo ""

# Mostrar resumo rápido
echo "📊 Resumo Rápido dos Resultados:"
echo "================================"

for test_name in "baseline_low" "typical_load" "high_load" "burst_test" "sustained_load" "low_latency" "high_throughput"; do
    latest_file=$(ls -t "$RESULTS_DIR"/${test_name}_*.json 2>/dev/null | head -1 || echo "")
    if [[ -n "$latest_file" && -f "$latest_file" ]]; then
        echo -n "🔸 $test_name: "
        python3 -c "
import json
try:
    data = json.load(open('$latest_file'))
    gm = data['global_metrics']
    print(f\"{gm['actual_ops_per_second']:.0f} ops/sec, {gm['success_rate']:.1f}% success, P95: {gm['p95_latency_ms']:.1f}ms\")
except:
    print('Erro ao ler resultados')
" 2>/dev/null || echo "Erro ao processar"
    fi
done

echo ""
echo "🎯 Próximos passos:"
echo "1. Analisar resultados detalhados nos arquivos JSON"
echo "2. Identificar gargalos de performance"
echo "3. Implementar otimizações baseadas nos resultados"
echo "4. Comparar com benchmarks de referência (Redis, etc.)"
echo ""
echo "📁 Todos os resultados estão em: $RESULTS_DIR"