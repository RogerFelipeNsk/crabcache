#!/bin/bash

# Script para auxiliar na configuração do deploy automático Docker
# Este script não configura as secrets automaticamente (isso deve ser feito manualmente no GitHub)
# Mas fornece instruções e validações

set -e

echo "🐳 Configuração do Deploy Automático Docker para CrabCache"
echo "========================================================="
echo

# Verificar se estamos no diretório correto
if [ ! -f "Cargo.toml" ] || [ ! -d ".github/workflows" ]; then
    echo "❌ Erro: Execute este script no diretório raiz do projeto CrabCache"
    exit 1
fi

echo "✅ Diretório do projeto verificado"

# Verificar se Docker está instalado
if ! command -v docker &> /dev/null; then
    echo "❌ Docker não está instalado. Instale o Docker primeiro."
    exit 1
fi

echo "✅ Docker está instalado"

# Verificar se o Dockerfile existe
if [ ! -f "Dockerfile" ]; then
    echo "❌ Dockerfile não encontrado no diretório raiz"
    exit 1
fi

echo "✅ Dockerfile encontrado"

# Testar build local
echo
echo "🔨 Testando build local do Docker..."
if docker build -t crabcache-test . > /dev/null 2>&1; then
    echo "✅ Build local do Docker funcionando"
    docker rmi crabcache-test > /dev/null 2>&1
else
    echo "❌ Erro no build local do Docker. Verifique o Dockerfile."
    exit 1
fi

# Verificar workflows
echo
echo "📋 Verificando workflows do GitHub Actions..."

if [ -f ".github/workflows/ci.yml" ]; then
    echo "✅ Workflow CI encontrado"
    
    if grep -q "DOCKER_USERNAME" .github/workflows/ci.yml; then
        echo "✅ Deploy automático configurado no CI"
    else
        echo "❌ Deploy automático não configurado no CI"
    fi
else
    echo "❌ Workflow CI não encontrado"
fi

if [ -f ".github/workflows/release.yml" ]; then
    echo "✅ Workflow Release encontrado"
else
    echo "❌ Workflow Release não encontrado"
fi

echo
echo "🔑 PRÓXIMOS PASSOS - Configuração das Secrets:"
echo "=============================================="
echo
echo "1. Acesse: https://github.com/$(git config --get remote.origin.url | sed 's/.*github.com[:/]\([^.]*\).*/\1/')/settings/secrets/actions"
echo
echo "2. Adicione as seguintes secrets:"
echo "   - DOCKER_USERNAME: seu nome de usuário do Docker Hub"
echo "   - DOCKER_PASSWORD: sua senha ou Personal Access Token do Docker Hub"
echo
echo "3. Para criar um Personal Access Token (recomendado):"
echo "   - Acesse: https://hub.docker.com/settings/security"
echo "   - Clique em 'New Access Token'"
echo "   - Dê um nome descritivo e selecione as permissões necessárias"
echo
echo "4. Após configurar as secrets, faça um push para a branch main:"
echo "   git add ."
echo "   git commit -m 'feat: configure automatic docker deploy'"
echo "   git push origin main"
echo
echo "5. Verifique o deploy em: https://github.com/$(git config --get remote.origin.url | sed 's/.*github.com[:/]\([^.]*\).*/\1/')/actions"
echo

# Verificar se há mudanças não commitadas
if [ -n "$(git status --porcelain)" ]; then
    echo "⚠️  ATENÇÃO: Há mudanças não commitadas no repositório"
    echo "   Commit as mudanças antes de testar o deploy automático"
    echo
fi

echo "📚 Para mais informações, consulte: docs/DOCKER_DEPLOY_SETUP.md"
echo
echo "🎉 Setup concluído! Configure as secrets no GitHub e faça um push para testar."