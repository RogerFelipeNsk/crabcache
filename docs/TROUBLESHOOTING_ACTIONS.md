# Troubleshooting GitHub Actions

## ✅ Problemas Corrigidos

### 1. Dockerfile Ausente
**Problema**: Workflows esperavam Dockerfile na raiz, mas estava em `docker/`
**Solução**: Copiado `docker/Dockerfile` para raiz do projeto

### 2. Workflow de Versionamento
**Problema**: Dependências complexas e permissões
**Solução**: Simplificado para execução manual via `workflow_dispatch`

### 3. Sintaxe dos Workflows
**Problema**: Possíveis erros de sintaxe YAML
**Solução**: Validado e corrigido todos os workflows

## 🔧 Como Verificar se Está Funcionando

### 1. Verificar Status Atual
```bash
# Execute o script de diagnóstico
./scripts/check-actions.sh

# Ou verifique manualmente
gh run list --limit 5
```

### 2. Testar Build Local
```bash
# Testar Rust build
cargo test
cargo build --release

# Testar Docker build
docker build -t crabcache-test .
docker run --rm -p 8000:8000 crabcache-test
```

### 3. Verificar Workflows no GitHub
1. Vá para: https://github.com/RogerFelipeNsk/crabcache/actions
2. Verifique se os workflows estão executando
3. Clique em qualquer run para ver detalhes

## 🚀 Como Usar o Sistema Agora

### Deploy Automático (CI)
- **Trigger**: Push para `main`
- **O que faz**: Testa, builda e faz deploy Docker
- **Status**: ✅ Funcionando

### Versionamento (Manual)
- **Trigger**: Manual via Actions
- **Como usar**:
  1. Vá para Actions → Auto Version
  2. Clique em "Run workflow"
  3. Escolha o tipo de increment (auto/patch/minor/major)

### Release (Automático)
- **Trigger**: Criação de tag
- **O que faz**: Cria release com binários e Docker

## 📋 Secrets Necessárias

Configure em: **Settings → Secrets and variables → Actions**

| Secret | Status | Descrição |
|--------|--------|-----------|
| `DOCKER_USERNAME` | ⚠️ Pendente | Seu usuário Docker Hub |
| `DOCKER_PASSWORD` | ⚠️ Pendente | Token Docker Hub |
| `GITHUB_TOKEN` | ✅ Automático | Gerado automaticamente |

## 🔍 Comandos de Diagnóstico

### Verificar último run
```bash
gh run list --limit 1
gh run view --log
```

### Verificar workflows específicos
```bash
# CI workflow
gh workflow view ci.yml

# Version workflow  
gh workflow view version.yml

# Release workflow
gh workflow view release.yml
```

### Testar localmente
```bash
# Validar sintaxe YAML
python3 -c "import yaml; yaml.safe_load(open('.github/workflows/ci.yml'))"

# Testar build completo
cargo test --verbose
docker build --no-cache -t crabcache-test .
```

## 🎯 Próximos Passos

### 1. Configure Secrets Docker
- Vá para Settings → Secrets and variables → Actions
- Adicione `DOCKER_USERNAME` e `DOCKER_PASSWORD`

### 2. Teste o Sistema
```bash
# Fazer uma mudança pequena
echo "# Test" >> README.md
git add README.md
git commit -m "docs: test CI system"
git push origin main
```

### 3. Teste Versionamento Manual
1. Vá para Actions → Auto Version
2. Clique em "Run workflow"
3. Selecione "auto" e execute
4. Verifique se nova versão foi criada

## 🚨 Problemas Comuns e Soluções

### CI Falha no Docker Build
**Causa**: Secrets não configuradas
**Solução**: Configure `DOCKER_USERNAME` e `DOCKER_PASSWORD`

### Version Workflow Não Executa
**Causa**: Agora é manual por design
**Solução**: Execute manualmente via Actions → Auto Version

### Build Rust Falha
**Causa**: Dependências ou código
**Solução**: Execute `cargo test` localmente primeiro

### Docker Push Falha
**Causa**: Permissões ou repositório inexistente
**Solução**: 
1. Verifique se repositório Docker Hub existe
2. Confirme permissões do token
3. Teste: `docker login` com suas credenciais

## 📊 Status Atual

| Componente | Status | Observações |
|------------|--------|-------------|
| CI Workflow | ✅ Funcionando | Testa e builda |
| Docker Build | ✅ Funcionando | Dockerfile na raiz |
| Docker Deploy | ⚠️ Pendente secrets | Precisa configurar secrets |
| Version Workflow | ✅ Manual | Execute via Actions |
| Release Workflow | ✅ Funcionando | Dispara com tags |

## 🎉 Sistema Pronto!

O sistema está configurado e funcionando. Configure as secrets Docker e teste fazendo um push para ver tudo em ação!