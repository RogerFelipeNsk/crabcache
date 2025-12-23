#!/bin/bash

# Script para iniciar CrabCache com Docker Compose
# Uso: ./scripts/docker-start.sh [--build] [--test] [--logs]

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"

echo "🐳 CrabCache Docker Compose Starter"
echo "==================================="

# Parse argumentos
BUILD=false
TEST=false
LOGS=false
DETACH=true

for arg in "$@"; do
    case $arg in
        --build)
            BUILD=true
            shift
            ;;
        --test)
            TEST=true
            shift
            ;;
        --logs)
            LOGS=true
            DETACH=false
            shift
            ;;
        --help)
            echo "Uso: $0 [opções]"
            echo ""
            echo "Opções:"
            echo "  --build    Rebuild das imagens Docker"
            echo "  --test     Incluir serviço de teste de carga"
            echo "  --logs     Mostrar logs em tempo real"
            echo "  --help     Mostrar esta ajuda"
            exit 0
            ;;
        *)
            echo "❌ Argumento desconhecido: $arg"
            echo "Use --help para ver opções disponíveis"
            exit 1
            ;;
    esac
done

cd "$PROJECT_DIR"

# Parar serviços existentes
echo "🛑 Parando serviços existentes..."
docker-compose down --remove-orphans 2>/dev/null || true

# Build se solicitado
if [ "$BUILD" = true ]; then
    echo "🔨 Rebuilding imagens Docker..."
    docker-compose build --no-cache
fi

# Definir perfis
PROFILES=""
if [ "$TEST" = true ]; then
    PROFILES="--profile testing"
    echo "🧪 Incluindo serviços de teste"
fi

# Iniciar serviços
echo "🚀 Iniciando serviços..."
if [ "$DETACH" = true ]; then
    docker-compose $PROFILES up -d
else
    docker-compose $PROFILES up
fi

if [ "$DETACH" = true ]; then
    echo ""
    echo "⏳ Aguardando serviços ficarem prontos..."
    
    # Aguardar CrabCache
    echo -n "   CrabCache: "
    for i in {1..30}; do
        if curl -s http://localhost:7000 >/dev/null 2>&1 || nc -z localhost 7000 2>/dev/null; then
            echo "✅ Pronto"
            break
        fi
        echo -n "."
        sleep 2
    done
    
    # Aguardar HTTP Wrapper
    echo -n "   HTTP Wrapper: "
    for i in {1..30}; do
        if curl -s http://localhost:8000/health >/dev/null 2>&1; then
            echo "✅ Pronto"
            break
        fi
        echo -n "."
        sleep 2
    done
    
    echo ""
    echo "🎉 Serviços iniciados com sucesso!"
    echo "================================="
    echo "📊 CrabCache TCP:    localhost:7000"
    echo "🌐 HTTP Wrapper:     http://localhost:8000"
    echo "📋 Documentação:     http://localhost:8000/"
    echo ""
    echo "🔧 Comandos úteis:"
    echo "   docker-compose logs -f                    # Ver logs"
    echo "   docker-compose ps                         # Status dos serviços"
    echo "   docker-compose exec crabcache /bin/sh     # Shell no CrabCache"
    echo "   docker-compose down                       # Parar tudo"
    echo ""
    echo "🧪 Teste rápido:"
    echo "   curl http://localhost:8000/health"
    echo "   curl http://localhost:8000/ping"
    echo ""
    
    if [ "$TEST" = true ]; then
        echo "🚀 Executando teste de carga..."
        docker-compose exec load-tester python load_test.py
    fi
    
    if [ "$LOGS" = true ]; then
        echo "📋 Mostrando logs (Ctrl+C para sair):"
        docker-compose logs -f
    fi
fi