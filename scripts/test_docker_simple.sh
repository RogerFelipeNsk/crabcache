#!/bin/bash
# Script simples para testar CrabCache no Docker

set -e

HOST=${1:-localhost}
PORT=${2:-7000}

echo "🦀 Testando CrabCache em $HOST:$PORT"
echo "=================================="

# Função para enviar comando
send_cmd() {
    local cmd="$1"
    echo "➤ $cmd"
    echo "$cmd" | nc -w 1 $HOST $PORT
    echo ""
}

# Verificar se netcat está disponível
if ! command -v nc &> /dev/null; then
    echo "❌ netcat (nc) não encontrado. Instale com:"
    echo "   macOS: brew install netcat"
    echo "   Ubuntu: sudo apt install netcat"
    echo "   CentOS: sudo yum install nc"
    exit 1
fi

# Verificar conectividade
echo "🔍 Verificando conectividade..."
if ! nc -z $HOST $PORT; then
    echo "❌ Não foi possível conectar em $HOST:$PORT"
    echo ""
    echo "💡 Para rodar o CrabCache:"
    echo "   docker run -d --name crabcache -p 7000:7000 crabcache:latest"
    echo ""
    exit 1
fi

echo "✅ Conectado com sucesso!"
echo ""

# Testes básicos
echo "🏓 Teste 1: PING"
send_cmd "PING"

echo "📦 Teste 2: PUT/GET/DEL"
send_cmd "PUT hello world"
send_cmd "GET hello"
send_cmd "DEL hello"
send_cmd "GET hello"

echo "⏰ Teste 3: PUT com TTL"
send_cmd "PUT temp_key temp_value 5"
send_cmd "GET temp_key"

echo "📊 Teste 4: STATS"
send_cmd "STATS"

echo "🎯 Teste 5: Dados JSON"
send_cmd 'PUT user:1 {"name":"John","age":30}'
send_cmd "GET user:1"

echo "✅ Todos os testes concluídos!"
echo ""
echo "💡 Para mais testes detalhados:"
echo "   python3 docs/test_api.py $HOST $PORT"