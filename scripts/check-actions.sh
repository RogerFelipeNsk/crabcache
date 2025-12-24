#!/bin/bash

# Script para verificar status das GitHub Actions e diagnosticar problemas

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"

# Cores
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

echo "🔍 Diagnóstico GitHub Actions - CrabCache"
echo "========================================"
echo

# Verificar se estamos no repositório correto
cd "$PROJECT_DIR"

if ! git remote get-url origin | grep -q "crabcache"; then
    echo -e "${RED}❌ Não parece ser o repositório CrabCache${NC}"
    exit 1
fi

REPO_URL=$(git remote get-url origin | sed 's/.*github.com[:/]\([^.]*\).*/\1/')
echo -e "${BLUE}📁 Repositório: $REPO_URL${NC}"
echo

# Verificar último commit
LAST_COMMIT=$(git log -1 --oneline)
echo -e "${BLUE}📝 Último commit: $LAST_COMMIT${NC}"
echo

# Verificar workflows
echo "🔧 Verificando workflows..."
echo

if [ -f ".github/workflows/ci.yml" ]; then
    echo -e "${GREEN}✅ CI workflow encontrado${NC}"
else
    echo -e "${RED}❌ CI workflow não encontrado${NC}"
fi

if [ -f ".github/workflows/version.yml" ]; then
    echo -e "${GREEN}✅ Version workflow encontrado${NC}"
else
    echo -e "${RED}❌ Version workflow não encontrado${NC}"
fi

if [ -f ".github/workflows/release.yml" ]; then
    echo -e "${GREEN}✅ Release workflow encontrado${NC}"
else
    echo -e "${RED}❌ Release workflow não encontrado${NC}"
fi

echo

# Verificar sintaxe dos workflows
echo "🔍 Verificando sintaxe dos workflows..."
echo

for workflow in .github/workflows/*.yml; do
    if [ -f "$workflow" ]; then
        workflow_name=$(basename "$workflow")
        if python3 -c "import yaml; yaml.safe_load(open('$workflow'))" 2>/dev/null; then
            echo -e "${GREEN}✅ $workflow_name - sintaxe OK${NC}"
        else
            echo -e "${RED}❌ $workflow_name - erro de sintaxe${NC}"
            echo "   Execute: python3 -c \"import yaml; yaml.safe_load(open('$workflow'))\" para detalhes"
        fi
    fi
done

echo

# Verificar Dockerfile
echo "🐳 Verificando Dockerfile..."
if [ -f "Dockerfile" ]; then
    echo -e "${GREEN}✅ Dockerfile encontrado${NC}"
    
    # Verificar sintaxe básica
    if docker build --dry-run . > /dev/null 2>&1; then
        echo -e "${GREEN}✅ Dockerfile sintaxe OK${NC}"
    else
        echo -e "${YELLOW}⚠️  Dockerfile pode ter problemas (teste: docker build .)${NC}"
    fi
else
    echo -e "${RED}❌ Dockerfile não encontrado${NC}"
fi

echo

# Verificar Cargo.toml
echo "📦 Verificando Cargo.toml..."
if [ -f "Cargo.toml" ]; then
    echo -e "${GREEN}✅ Cargo.toml encontrado${NC}"
    
    # Verificar sintaxe
    if cargo check --dry-run > /dev/null 2>&1; then
        echo -e "${GREEN}✅ Cargo.toml sintaxe OK${NC}"
    else
        echo -e "${YELLOW}⚠️  Cargo.toml pode ter problemas (teste: cargo check)${NC}"
    fi
    
    # Mostrar versão atual
    CURRENT_VERSION=$(grep '^version = ' Cargo.toml | sed 's/version = "\(.*\)"/\1/')
    echo -e "${BLUE}📋 Versão atual: $CURRENT_VERSION${NC}"
else
    echo -e "${RED}❌ Cargo.toml não encontrado${NC}"
fi

echo

# Verificar possíveis problemas comuns
echo "🚨 Verificando problemas comuns..."
echo

# 1. Verificar se há secrets necessárias (não podemos ver, mas podemos avisar)
echo -e "${YELLOW}⚠️  Secrets necessárias (configure no GitHub):${NC}"
echo "   - DOCKER_USERNAME (seu usuário Docker Hub)"
echo "   - DOCKER_PASSWORD (token Docker Hub)"
echo

# 2. Verificar se há arquivos que podem causar problemas
if [ -f "Cargo.lock" ]; then
    echo -e "${GREEN}✅ Cargo.lock presente${NC}"
else
    echo -e "${YELLOW}⚠️  Cargo.lock não encontrado (será gerado automaticamente)${NC}"
fi

# 3. Verificar se há testes
if [ -d "tests" ] || grep -q "\[\[bin\]\]" Cargo.toml || grep -q "test" src/main.rs 2>/dev/null; then
    echo -e "${GREEN}✅ Testes encontrados${NC}"
else
    echo -e "${YELLOW}⚠️  Nenhum teste encontrado${NC}"
fi

echo

# Links úteis
echo "🔗 Links úteis:"
echo "   Actions: https://github.com/$REPO_URL/actions"
echo "   Settings: https://github.com/$REPO_URL/settings/secrets/actions"
echo "   Releases: https://github.com/$REPO_URL/releases"
echo

# Comandos para verificar logs
echo "📋 Comandos para verificar problemas:"
echo
echo "# Verificar último workflow run:"
echo "gh run list --limit 5"
echo
echo "# Ver logs do último run:"
echo "gh run view --log"
echo
echo "# Ver logs de um workflow específico:"
echo "gh run view [RUN_ID] --log"
echo
echo "# Testar build local:"
echo "cargo test"
echo "docker build -t crabcache-test ."
echo
echo "# Verificar sintaxe workflows:"
echo "python3 -c \"import yaml; [yaml.safe_load(open(f)) for f in ['.github/workflows/ci.yml', '.github/workflows/version.yml', '.github/workflows/release.yml']]\""
echo

echo -e "${BLUE}💡 Dica: Se você tem GitHub CLI instalado, use os comandos 'gh' acima para ver os logs detalhados${NC}"