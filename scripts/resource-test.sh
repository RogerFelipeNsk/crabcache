#!/bin/bash

# Script para testar CrabCache com diferentes limites de recursos
# Uso: ./scripts/resource-test.sh [low|medium|high]

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"

PROFILE=${1:-medium}

echo "🔬 CrabCache Resource Limit Tester"
echo "=================================="
echo "Perfil de recursos: $PROFILE"
echo ""

cd "$PROJECT_DIR"

# Definir limites baseados no perfil
case $PROFILE in
    low)
        CRABCACHE_CPU="0.5"
        CRABCACHE_MEMORY="128M"
        WRAPPER_CPU="0.25"
        WRAPPER_MEMORY="64M"
        TEST_USERS="5"
        TEST_DURATION="30"
        echo "📊 Perfil LOW: Recursos muito limitados"
        ;;
    medium)
        CRABCACHE_CPU="1.0"
        CRABCACHE_MEMORY="512M"
        WRAPPER_CPU="0.5"
        WRAPPER_MEMORY="128M"
        TEST_USERS="10"
        TEST_DURATION="60"
        echo "📊 Perfil MEDIUM: Recursos moderados"
        ;;
    high)
        CRABCACHE_CPU="2.0"
        CRABCACHE_MEMORY="1G"
        WRAPPER_CPU="1.0"
        WRAPPER_MEMORY="256M"
        TEST_USERS="20"
        TEST_DURATION="120"
        echo "📊 Perfil HIGH: Recursos abundantes"
        ;;
    *)
        echo "❌ Perfil inválido: $PROFILE"
        echo "Use: low, medium, ou high"
        exit 1
        ;;
esac

# Criar docker-compose override para este teste
cat > docker-compose.override.yml << EOF
version: '3.8'

services:
  crabcache:
    deploy:
      resources:
        limits:
          cpus: '$CRABCACHE_CPU'
          memory: $CRABCACHE_MEMORY
        reservations:
          cpus: '$(echo "$CRABCACHE_CPU * 0.5" | bc)'
          memory: $(echo "$CRABCACHE_MEMORY" | sed 's/G/000M/g' | sed 's/M//g' | awk '{print int($1/2)"M"}')

  http-wrapper:
    deploy:
      resources:
        limits:
          cpus: '$WRAPPER_CPU'
          memory: $WRAPPER_MEMORY
        reservations:
          cpus: '$(echo "$WRAPPER_CPU * 0.5" | bc)'
          memory: $(echo "$WRAPPER_MEMORY" | sed 's/M//g' | awk '{print int($1/2)"M"}')

  load-tester:
    environment:
      - CONCURRENT_USERS=$TEST_USERS
      - TEST_DURATION=$TEST_DURATION
    profiles:
      - testing
EOF

echo "📝 Configuração de recursos:"
echo "   CrabCache: CPU=$CRABCACHE_CPU, Memory=$CRABCACHE_MEMORY"
echo "   Wrapper: CPU=$WRAPPER_CPU, Memory=$WRAPPER_MEMORY"
echo "   Teste: Users=$TEST_USERS, Duration=${TEST_DURATION}s"
echo ""

# Parar serviços existentes
echo "🛑 Parando serviços existentes..."
docker-compose down --remove-orphans 2>/dev/null || true

# Iniciar com perfil de teste
echo "🚀 Iniciando serviços com limites de recursos..."
docker-compose --profile testing up -d

# Aguardar serviços
echo "⏳ Aguardando serviços ficarem prontos..."
sleep 10

# Verificar se estão rodando
if ! curl -s http://localhost:8000/health >/dev/null; then
    echo "❌ Serviços não estão respondendo"
    docker-compose logs
    exit 1
fi

echo "✅ Serviços prontos!"
echo ""

# Executar teste de baseline
echo "📊 Executando teste de baseline..."
curl -s http://localhost:8000/stats | python3 -m json.tool

echo ""
echo "🚀 Iniciando teste de carga..."
echo "   Usuários: $TEST_USERS"
echo "   Duração: ${TEST_DURATION}s"
echo ""

# Executar teste de carga
docker-compose exec -T load-tester python load_test.py

echo ""
echo "📈 Estatísticas finais:"
curl -s http://localhost:8000/stats | python3 -m json.tool

echo ""
echo "🐳 Uso de recursos dos containers:"
docker stats --no-stream --format "table {{.Container}}\t{{.CPUPerc}}\t{{.MemUsage}}\t{{.MemPerc}}"

echo ""
echo "💾 Logs dos últimas 20 linhas:"
echo "--- CrabCache ---"
docker-compose logs --tail=20 crabcache

echo ""
echo "--- HTTP Wrapper ---"
docker-compose logs --tail=20 http-wrapper

# Cleanup
echo ""
echo "🧹 Limpando configuração temporária..."
rm -f docker-compose.override.yml

echo ""
echo "✅ Teste de recursos concluído!"
echo "   Para parar os serviços: docker-compose down"