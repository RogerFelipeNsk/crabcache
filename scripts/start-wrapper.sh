#!/bin/bash

# Script para iniciar CrabCache + HTTP Wrapper
# Uso: ./scripts/start-wrapper.sh [local|docker]

set -e

MODE=${1:-local}

echo "🦀 CrabCache + HTTP Wrapper Starter"
echo "=================================="

# Função para verificar se uma porta está em uso
check_port() {
    local port=$1
    if lsof -Pi :$port -sTCP:LISTEN -t >/dev/null 2>&1; then
        return 0  # Porta em uso
    else
        return 1  # Porta livre
    fi
}

# Função para aguardar porta ficar disponível
wait_for_port() {
    local host=$1
    local port=$2
    local timeout=${3:-30}
    
    echo "⏳ Aguardando $host:$port ficar disponível..."
    
    for i in $(seq 1 $timeout); do
        if nc -z $host $port 2>/dev/null; then
            echo "✅ $host:$port está disponível!"
            return 0
        fi
        sleep 1
    done
    
    echo "❌ Timeout aguardando $host:$port"
    return 1
}

# Função para parar processos
cleanup() {
    echo ""
    echo "🛑 Parando serviços..."
    
    if [[ -n $CRABCACHE_PID ]]; then
        kill $CRABCACHE_PID 2>/dev/null || true
        echo "   CrabCache parado"
    fi
    
    if [[ -n $WRAPPER_PID ]]; then
        kill $WRAPPER_PID 2>/dev/null || true
        echo "   HTTP Wrapper parado"
    fi
    
    if [[ $MODE == "docker" && -n $DOCKER_CONTAINER ]]; then
        docker stop $DOCKER_CONTAINER 2>/dev/null || true
        echo "   Docker container parado"
    fi
    
    exit 0
}

# Configurar trap para cleanup
trap cleanup SIGINT SIGTERM

if [[ $MODE == "local" ]]; then
    echo "📍 Modo: Local Development"
    echo ""
    
    # Verificar se as portas estão livres
    if check_port 7000; then
        echo "❌ Porta 7000 já está em uso. Pare o processo existente primeiro."
        exit 1
    fi
    
    if check_port 8000; then
        echo "❌ Porta 8000 já está em uso. Pare o processo existente primeiro."
        exit 1
    fi
    
    # Compilar CrabCache se necessário
    echo "🔨 Compilando CrabCache..."
    cd crabcache
    cargo build --release
    cd ..
    
    # Iniciar CrabCache
    echo "🚀 Iniciando CrabCache na porta 7000..."
    cd crabcache
    ./target/release/crabcache &
    CRABCACHE_PID=$!
    cd ..
    
    # Aguardar CrabCache ficar disponível
    wait_for_port localhost 7000
    
    # Iniciar HTTP Wrapper
    echo "🌐 Iniciando HTTP Wrapper na porta 8000..."
    cd crabcache
    python3 http_wrapper.py &
    WRAPPER_PID=$!
    cd ..
    
    # Aguardar HTTP Wrapper ficar disponível
    wait_for_port localhost 8000
    
elif [[ $MODE == "docker" ]]; then
    echo "📍 Modo: Docker Container"
    echo ""
    
    # Verificar se a porta 8000 está livre
    if check_port 8000; then
        echo "❌ Porta 8000 já está em uso. Pare o processo existente primeiro."
        exit 1
    fi
    
    # Construir imagem Docker se necessário
    echo "🐳 Construindo imagem Docker..."
    cd crabcache
    docker build -t crabcache:latest .
    cd ..
    
    # Iniciar container CrabCache
    echo "🚀 Iniciando CrabCache no Docker (porta 7004 -> 7000)..."
    DOCKER_CONTAINER=$(docker run -d -p 7004:7000 crabcache:latest)
    echo "   Container ID: $DOCKER_CONTAINER"
    
    # Aguardar CrabCache ficar disponível
    wait_for_port localhost 7004
    
    # Iniciar HTTP Wrapper apontando para Docker
    echo "🌐 Iniciando HTTP Wrapper na porta 8000 (conectando ao Docker)..."
    cd crabcache
    CRABCACHE_HOST=localhost CRABCACHE_PORT=7005 python3 http_wrapper.py &
    WRAPPER_PID=$!
    cd ..
    
    # Aguardar HTTP Wrapper ficar disponível
    wait_for_port localhost 8000
    
else
    echo "❌ Modo inválido: $MODE"
    echo "   Use: ./scripts/start-wrapper.sh [local|docker]"
    exit 1
fi

echo ""
echo "🎉 Serviços iniciados com sucesso!"
echo "=================================="
echo "📊 CrabCache TCP: localhost:$([ $MODE == 'docker' ] && echo '7005' || echo '7000')"
echo "🌐 HTTP Wrapper:  http://localhost:8000"
echo ""
echo "📖 Endpoints disponíveis:"
echo "   GET  /health          - Status do wrapper"
echo "   GET  /ping            - PING do CrabCache"
echo "   POST /put             - PUT key/value"
echo "   GET  /get/<key>       - GET key"
echo "   DELETE /delete/<key>  - DEL key"
echo "   POST /expire          - EXPIRE key"
echo "   GET  /stats           - STATS do servidor"
echo "   POST /command         - Comando raw"
echo ""
echo "🔧 Teste rápido:"
echo "   curl http://localhost:8000/health"
echo "   curl http://localhost:8000/ping"
echo ""
echo "📋 Insomnia Collection: docs/insomnia-collection.json"
echo "📚 Documentação: docs/API.md"
echo ""
echo "⏹️  Pressione Ctrl+C para parar os serviços"

# Aguardar indefinidamente
while true; do
    sleep 1
done