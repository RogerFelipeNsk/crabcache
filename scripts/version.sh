#!/bin/bash

# Script para gerenciar versões do CrabCache
# Suporta conventional commits e semantic versioning

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"

# Cores para output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Função para mostrar ajuda
show_help() {
    echo "🏷️  CrabCache Version Manager"
    echo "=========================="
    echo
    echo "Uso: $0 [COMANDO] [OPÇÕES]"
    echo
    echo "Comandos:"
    echo "  current           Mostra a versão atual"
    echo "  next [tipo]       Mostra qual seria a próxima versão"
    echo "  bump [tipo]       Incrementa a versão e cria tag"
    echo "  changelog         Gera changelog desde a última tag"
    echo "  help              Mostra esta ajuda"
    echo
    echo "Tipos de versão:"
    echo "  patch             Incrementa versão patch (0.0.X)"
    echo "  minor             Incrementa versão minor (0.X.0)"
    echo "  major             Incrementa versão major (X.0.0)"
    echo "  auto              Detecta automaticamente baseado nos commits"
    echo
    echo "Exemplos:"
    echo "  $0 current"
    echo "  $0 next auto"
    echo "  $0 bump patch"
    echo "  $0 changelog"
    echo
    echo "Conventional Commits suportados:"
    echo "  feat:     nova funcionalidade (minor)"
    echo "  fix:      correção de bug (patch)"
    echo "  perf:     melhoria de performance (patch)"
    echo "  docs:     documentação (patch)"
    echo "  BREAKING: mudança que quebra compatibilidade (major)"
}

# Função para obter versão atual
get_current_version() {
    grep '^version = ' "$PROJECT_DIR/Cargo.toml" | sed 's/version = "\(.*\)"/\1/'
}

# Função para obter última tag
get_last_tag() {
    cd "$PROJECT_DIR"
    git describe --tags --abbrev=0 2>/dev/null || echo "v0.0.0"
}

# Função para analisar commits e determinar tipo de increment
analyze_commits() {
    cd "$PROJECT_DIR"
    local last_tag=$(get_last_tag)
    local commits=$(git log ${last_tag}..HEAD --oneline --pretty=format:"%s" 2>/dev/null || echo "")
    
    if [ -z "$commits" ]; then
        echo "patch"
        return
    fi
    
    # Verificar BREAKING CHANGES
    if echo "$commits" | grep -qE "^(feat|fix|perf|docs)(\(.+\))?!:|^BREAKING CHANGE:|^[^:]+!:"; then
        echo "major"
        return
    fi
    
    # Verificar novas features
    if echo "$commits" | grep -qE "^feat(\(.+\))?:"; then
        echo "minor"
        return
    fi
    
    # Verificar fixes ou outras mudanças
    if echo "$commits" | grep -qE "^(fix|perf|docs)(\(.+\))?:"; then
        echo "patch"
        return
    fi
    
    # Default para patch
    echo "patch"
}

# Função para calcular próxima versão
calculate_next_version() {
    local current_version="$1"
    local increment_type="$2"
    
    IFS='.' read -ra VERSION_PARTS <<< "$current_version"
    local major=${VERSION_PARTS[0]}
    local minor=${VERSION_PARTS[1]}
    local patch=${VERSION_PARTS[2]}
    
    case $increment_type in
        "major")
            major=$((major + 1))
            minor=0
            patch=0
            ;;
        "minor")
            minor=$((minor + 1))
            patch=0
            ;;
        "patch")
            patch=$((patch + 1))
            ;;
        *)
            echo "Tipo de increment inválido: $increment_type" >&2
            exit 1
            ;;
    esac
    
    echo "$major.$minor.$patch"
}

# Função para gerar changelog
generate_changelog() {
    cd "$PROJECT_DIR"
    local last_tag=$(get_last_tag)
    local commits=$(git log ${last_tag}..HEAD --oneline --pretty=format:"%s" 2>/dev/null || echo "")
    
    if [ -z "$commits" ]; then
        echo "Nenhum commit novo desde $last_tag"
        return
    fi
    
    echo "## Mudanças desde $last_tag"
    echo
    
    # Categorizar commits
    local has_features=false
    local has_fixes=false
    local has_perf=false
    local has_docs=false
    local has_breaking=false
    
    # Verificar se há commits de cada categoria
    if echo "$commits" | grep -qE "^feat(\(.+\))?:"; then has_features=true; fi
    if echo "$commits" | grep -qE "^fix(\(.+\))?:"; then has_fixes=true; fi
    if echo "$commits" | grep -qE "^perf(\(.+\))?:"; then has_perf=true; fi
    if echo "$commits" | grep -qE "^docs(\(.+\))?:"; then has_docs=true; fi
    if echo "$commits" | grep -qE "^(feat|fix|perf|docs)(\(.+\))?!:|^BREAKING CHANGE:|^[^:]+!:"; then has_breaking=true; fi
    
    # BREAKING CHANGES
    if [ "$has_breaking" = true ]; then
        echo "### 💥 BREAKING CHANGES"
        echo "$commits" | grep -E "^(feat|fix|perf|docs)(\(.+\))?!:|^BREAKING CHANGE:|^[^:]+!:" | while read -r commit; do
            echo "- $(echo "$commit" | sed 's/^[^:]*!*: *//')"
        done
        echo
    fi
    
    # Features
    if [ "$has_features" = true ]; then
        echo "### ✨ Features"
        echo "$commits" | grep -E "^feat(\(.+\))?:" | grep -v "!" | while read -r commit; do
            echo "- $(echo "$commit" | sed 's/^feat[^:]*: *//')"
        done
        echo
    fi
    
    # Bug Fixes
    if [ "$has_fixes" = true ]; then
        echo "### 🐛 Bug Fixes"
        echo "$commits" | grep -E "^fix(\(.+\))?:" | grep -v "!" | while read -r commit; do
            echo "- $(echo "$commit" | sed 's/^fix[^:]*: *//')"
        done
        echo
    fi
    
    # Performance
    if [ "$has_perf" = true ]; then
        echo "### ⚡ Performance"
        echo "$commits" | grep -E "^perf(\(.+\))?:" | grep -v "!" | while read -r commit; do
            echo "- $(echo "$commit" | sed 's/^perf[^:]*: *//')"
        done
        echo
    fi
    
    # Documentation
    if [ "$has_docs" = true ]; then
        echo "### 📚 Documentation"
        echo "$commits" | grep -E "^docs(\(.+\))?:" | grep -v "!" | while read -r commit; do
            echo "- $(echo "$commit" | sed 's/^docs[^:]*: *//')"
        done
        echo
    fi
}

# Função para fazer bump da versão
bump_version() {
    local increment_type="$1"
    local current_version=$(get_current_version)
    
    if [ "$increment_type" = "auto" ]; then
        increment_type=$(analyze_commits)
        echo -e "${BLUE}Tipo detectado automaticamente: $increment_type${NC}"
    fi
    
    local new_version=$(calculate_next_version "$current_version" "$increment_type")
    
    echo -e "${YELLOW}Versão atual: $current_version${NC}"
    echo -e "${GREEN}Nova versão: $new_version${NC}"
    echo
    
    # Confirmar com usuário
    read -p "Confirma o bump da versão? (y/N): " -n 1 -r
    echo
    if [[ ! $REPLY =~ ^[Yy]$ ]]; then
        echo "Operação cancelada."
        exit 0
    fi
    
    cd "$PROJECT_DIR"
    
    # Verificar se há mudanças não commitadas
    if [ -n "$(git status --porcelain)" ]; then
        echo -e "${RED}Erro: Há mudanças não commitadas. Commit ou stash antes de continuar.${NC}"
        exit 1
    fi
    
    # Instalar cargo-edit se necessário
    if ! command -v cargo-set-version &> /dev/null; then
        echo "Instalando cargo-edit..."
        cargo install cargo-edit
    fi
    
    # Atualizar versão
    cargo set-version "$new_version"
    
    # Atualizar Cargo.lock
    cargo check > /dev/null 2>&1
    
    # Gerar changelog
    echo "Gerando changelog..."
    generate_changelog > "CHANGELOG_v${new_version}.md"
    
    # Commit e tag
    git add Cargo.toml Cargo.lock "CHANGELOG_v${new_version}.md"
    git commit -m "chore: bump version to $new_version"
    git tag "v$new_version"
    
    echo -e "${GREEN}✅ Versão atualizada para $new_version${NC}"
    echo -e "${BLUE}📝 Changelog gerado em CHANGELOG_v${new_version}.md${NC}"
    echo
    echo "Para publicar:"
    echo "  git push origin main"
    echo "  git push origin v$new_version"
}

# Main
case "${1:-help}" in
    "current")
        echo "Versão atual: $(get_current_version)"
        ;;
    "next")
        local increment_type="${2:-auto}"
        if [ "$increment_type" = "auto" ]; then
            increment_type=$(analyze_commits)
        fi
        local current_version=$(get_current_version)
        local next_version=$(calculate_next_version "$current_version" "$increment_type")
        echo "Versão atual: $current_version"
        echo "Próxima versão ($increment_type): $next_version"
        ;;
    "bump")
        local increment_type="${2:-auto}"
        bump_version "$increment_type"
        ;;
    "changelog")
        generate_changelog
        ;;
    "help"|*)
        show_help
        ;;
esac