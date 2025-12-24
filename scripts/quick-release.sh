#!/bin/bash

# Script rápido para fazer release sem depender de workflows complexos

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"

# Cores
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

echo -e "${BLUE}🚀 CrabCache Quick Release${NC}"
echo "========================="
echo

cd "$PROJECT_DIR"

# Verificar se estamos na main
CURRENT_BRANCH=$(git branch --show-current)
if [ "$CURRENT_BRANCH" != "main" ]; then
    echo -e "${RED}❌ Você deve estar na branch main para fazer release${NC}"
    echo "Branch atual: $CURRENT_BRANCH"
    exit 1
fi

# Verificar se há mudanças não commitadas
if [ -n "$(git status --porcelain)" ]; then
    echo -e "${RED}❌ Há mudanças não commitadas${NC}"
    echo "Commit ou stash suas mudanças primeiro"
    exit 1
fi

# Obter versão atual
CURRENT_VERSION=$(grep '^version = ' Cargo.toml | sed 's/version = "\(.*\)"/\1/')
echo -e "${BLUE}📋 Versão atual: $CURRENT_VERSION${NC}"

# Determinar próxima versão baseada nos commits
LAST_TAG=$(git describe --tags --abbrev=0 2>/dev/null || echo "v0.0.0")
echo -e "${BLUE}🏷️  Última tag: $LAST_TAG${NC}"

# Analisar commits
COMMITS=$(git log ${LAST_TAG}..HEAD --oneline --pretty=format:"%s" || echo "")

if [ -z "$COMMITS" ]; then
    echo -e "${YELLOW}⚠️  Nenhum commit novo desde $LAST_TAG${NC}"
    echo "Nada para fazer release"
    exit 0
fi

echo -e "${BLUE}📝 Commits desde $LAST_TAG:${NC}"
echo "$COMMITS" | head -5
if [ $(echo "$COMMITS" | wc -l) -gt 5 ]; then
    echo "... e mais $(( $(echo "$COMMITS" | wc -l) - 5 )) commits"
fi
echo

# Determinar tipo de increment
if echo "$COMMITS" | grep -qE "^(feat|feature)(\(.+\))?!:|^BREAKING CHANGE:|^[^:]+!:"; then
    INCREMENT_TYPE="major"
    echo -e "${RED}💥 BREAKING CHANGE detectado - incremento major${NC}"
elif echo "$COMMITS" | grep -qE "^(feat|feature)(\(.+\))?:"; then
    INCREMENT_TYPE="minor"
    echo -e "${GREEN}✨ Nova feature detectada - incremento minor${NC}"
elif echo "$COMMITS" | grep -qE "^(fix|bugfix)(\(.+\))?:"; then
    INCREMENT_TYPE="patch"
    echo -e "${YELLOW}🐛 Bug fix detectado - incremento patch${NC}"
else
    INCREMENT_TYPE="patch"
    echo -e "${BLUE}📦 Incremento patch padrão${NC}"
fi

# Calcular nova versão
IFS='.' read -ra VERSION_PARTS <<< "$CURRENT_VERSION"
MAJOR=${VERSION_PARTS[0]}
MINOR=${VERSION_PARTS[1]}
PATCH=${VERSION_PARTS[2]}

case $INCREMENT_TYPE in
    "major")
        MAJOR=$((MAJOR + 1))
        MINOR=0
        PATCH=0
        ;;
    "minor")
        MINOR=$((MINOR + 1))
        PATCH=0
        ;;
    "patch")
        PATCH=$((PATCH + 1))
        ;;
esac

NEW_VERSION="$MAJOR.$MINOR.$PATCH"
echo -e "${GREEN}🎯 Nova versão: $NEW_VERSION${NC}"
echo

# Confirmar
read -p "Confirma o release da versão $NEW_VERSION? (y/N): " -n 1 -r
echo
if [[ ! $REPLY =~ ^[Yy]$ ]]; then
    echo "Release cancelado"
    exit 0
fi

echo -e "${BLUE}🔧 Processando release...${NC}"

# Atualizar versão no Cargo.toml
sed -i.bak "s/^version = \".*\"/version = \"$NEW_VERSION\"/" Cargo.toml
rm Cargo.toml.bak

# Atualizar Cargo.lock
cargo check > /dev/null 2>&1

# Gerar changelog
CHANGELOG_FILE="CHANGELOG_v${NEW_VERSION}.md"
echo "## [$NEW_VERSION] - $(date +%Y-%m-%d)" > "$CHANGELOG_FILE"
echo "" >> "$CHANGELOG_FILE"

# Categorizar commits
echo "$COMMITS" | while read -r commit; do
    if echo "$commit" | grep -qE "^feat(\(.+\))?:"; then
        echo "### ✨ Features" >> "$CHANGELOG_FILE"
        echo "- $(echo "$commit" | sed 's/^feat[^:]*: //')" >> "$CHANGELOG_FILE"
        echo "" >> "$CHANGELOG_FILE"
    elif echo "$commit" | grep -qE "^fix(\(.+\))?:"; then
        echo "### 🐛 Bug Fixes" >> "$CHANGELOG_FILE"
        echo "- $(echo "$commit" | sed 's/^fix[^:]*: //')" >> "$CHANGELOG_FILE"
        echo "" >> "$CHANGELOG_FILE"
    elif echo "$commit" | grep -qE "^perf(\(.+\))?:"; then
        echo "### ⚡ Performance" >> "$CHANGELOG_FILE"
        echo "- $(echo "$commit" | sed 's/^perf[^:]*: //')" >> "$CHANGELOG_FILE"
        echo "" >> "$CHANGELOG_FILE"
    elif echo "$commit" | grep -qE "^docs(\(.+\))?:"; then
        echo "### 📚 Documentation" >> "$CHANGELOG_FILE"
        echo "- $(echo "$commit" | sed 's/^docs[^:]*: //')" >> "$CHANGELOG_FILE"
        echo "" >> "$CHANGELOG_FILE"
    fi
done

# Commit e tag
git add Cargo.toml Cargo.lock "$CHANGELOG_FILE"
git commit -m "chore: release version $NEW_VERSION

$(cat "$CHANGELOG_FILE")"

git tag "v$NEW_VERSION"

echo -e "${GREEN}✅ Release $NEW_VERSION criado localmente${NC}"
echo -e "${BLUE}📝 Changelog: $CHANGELOG_FILE${NC}"
echo

# Perguntar se quer fazer push
read -p "Fazer push para GitHub? (y/N): " -n 1 -r
echo
if [[ $REPLY =~ ^[Yy]$ ]]; then
    echo -e "${BLUE}📤 Fazendo push...${NC}"
    git push origin main
    git push origin "v$NEW_VERSION"
    
    echo -e "${GREEN}🎉 Release $NEW_VERSION publicado!${NC}"
    echo
    echo "🔗 Links:"
    echo "   - Actions: https://github.com/$(git config --get remote.origin.url | sed 's/.*github.com[:/]\([^.]*\).*/\1/')/actions"
    echo "   - Releases: https://github.com/$(git config --get remote.origin.url | sed 's/.*github.com[:/]\([^.]*\).*/\1/')/releases"
    echo "   - Docker: https://hub.docker.com/r/{seu-usuario}/crabcache"
else
    echo -e "${YELLOW}⚠️  Release criado localmente mas não publicado${NC}"
    echo "Para publicar depois:"
    echo "  git push origin main"
    echo "  git push origin v$NEW_VERSION"
fi