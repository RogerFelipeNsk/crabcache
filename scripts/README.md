# CrabCache Development Scripts

Este diretório contém scripts para validar o código antes de fazer push para o repositório.

## Scripts Disponíveis

### `quick-check.sh` - Verificação Rápida
Executa as verificações essenciais que devem passar antes de qualquer push:

```bash
./scripts/quick-check.sh
```

**O que verifica:**
- ✅ Formatação do código (`cargo fmt`)
- ✅ Build do projeto (`cargo build`)
- ✅ Testes básicos (com tolerância a falhas)

**Tempo:** ~30 segundos

### `pre-push-check.sh` - Validação Completa do CI
Executa todas as verificações que o CI fará, garantindo que o push passará:

```bash
./scripts/pre-push-check.sh
```

**O que verifica:**
- ✅ Formatação do código
- ✅ Linting com Clippy (sem warnings)
- ✅ Build release
- ✅ Todos os testes unitários
- ✅ Testes de integração
- ✅ Geração de documentação
- ✅ Auditoria de segurança (se cargo-audit estiver instalado)

**Tempo:** ~2-5 minutos

## Fluxo de Trabalho Recomendado

### Para desenvolvimento rápido:
```bash
# Faça suas alterações
vim src/some_file.rs

# Verificação rápida
./scripts/quick-check.sh

# Se passou, pode fazer commit
git add .
git commit -m "feat: sua alteração"
```

### Antes de push para main:
```bash
# Verificação completa
./scripts/pre-push-check.sh

# Se tudo passou, pode fazer push
git push origin main
```

## Instalação de Dependências Opcionais

Para auditoria de segurança:
```bash
cargo install cargo-audit
```

## Problemas Comuns

### Erro de formatação
```bash
# Corrige automaticamente
cargo fmt --all
```

### Warnings do Clippy
```bash
# Vê os warnings
cargo clippy --all-targets --all-features

# Aplica correções automáticas quando possível
cargo fix --lib --allow-dirty
```

### Testes falhando
```bash
# Executa testes específicos
cargo test test_name

# Executa com output detalhado
cargo test -- --nocapture
```

## Integração com Git Hooks

Para automatizar, você pode adicionar um git hook:

```bash
# Cria o hook pre-push
echo '#!/bin/bash\n./scripts/quick-check.sh' > .git/hooks/pre-push
chmod +x .git/hooks/pre-push
```

Agora toda vez que você fizer `git push`, o script será executado automaticamente.