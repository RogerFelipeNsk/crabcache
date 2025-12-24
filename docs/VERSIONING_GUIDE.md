# Guia de Versionamento Automático

O CrabCache usa **Semantic Versioning** com **Conventional Commits** para versionamento automático.

## Como Funciona

### 1. Versionamento Automático
- **Push para main**: Dispara análise automática dos commits
- **Conventional Commits**: Determina o tipo de increment (major/minor/patch)
- **Tag automática**: Cria tag e release no GitHub
- **Docker automático**: Publica nova versão no Docker Hub

### 2. Tipos de Commit e Incremento

| Tipo de Commit | Incremento | Exemplo |
|----------------|------------|---------|
| `feat:` | **minor** (0.X.0) | `feat: add cache eviction policy` |
| `fix:` | **patch** (0.0.X) | `fix: resolve memory leak in cleanup` |
| `perf:` | **patch** (0.0.X) | `perf: optimize hash table lookup` |
| `docs:` | **patch** (0.0.X) | `docs: update API documentation` |
| `feat!:` ou `BREAKING CHANGE:` | **major** (X.0.0) | `feat!: change API response format` |

### 3. Workflows

#### Workflow Principal (`ci.yml`)
- Executa testes e build
- Faz deploy Docker
- Dispara versionamento automático

#### Workflow de Versionamento (`version.yml`)
- Analisa commits desde última tag
- Incrementa versão no `Cargo.toml`
- Cria tag e release no GitHub
- Gera changelog automático

## Uso Manual

### Script de Versionamento
```bash
# Ver versão atual
./scripts/version.sh current

# Ver próxima versão (detecta automaticamente)
./scripts/version.sh next auto

# Ver próxima versão (tipo específico)
./scripts/version.sh next patch
./scripts/version.sh next minor
./scripts/version.sh next major

# Fazer bump da versão
./scripts/version.sh bump auto
./scripts/version.sh bump patch

# Gerar changelog
./scripts/version.sh changelog
```

### Versionamento Manual via GitHub
1. Vá para **Actions** no GitHub
2. Selecione **Auto Version**
3. Clique em **Run workflow**
4. Escolha o tipo de incremento

## Conventional Commits

### Formato
```
<tipo>[escopo opcional]: <descrição>

[corpo opcional]

[rodapé opcional]
```

### Exemplos Práticos

#### Feature (minor)
```bash
git commit -m "feat: add Redis protocol compatibility"
git commit -m "feat(cache): implement LRU eviction policy"
```

#### Bug Fix (patch)
```bash
git commit -m "fix: resolve race condition in concurrent access"
git commit -m "fix(memory): prevent memory leak in cleanup process"
```

#### Breaking Change (major)
```bash
git commit -m "feat!: change API response format to JSON"
git commit -m "feat: add new cache backend

BREAKING CHANGE: removes support for old configuration format"
```

#### Performance (patch)
```bash
git commit -m "perf: optimize hash table operations"
git commit -m "perf(storage): reduce memory allocation overhead"
```

#### Documentation (patch)
```bash
git commit -m "docs: update installation guide"
git commit -m "docs(api): add examples for cache operations"
```

## Controle de Versionamento

### Pular Versionamento
Adicione `[skip-version]` na mensagem do commit:
```bash
git commit -m "chore: update dependencies [skip-version]"
```

### Pular CI Completo
Adicione `[skip-ci]` na mensagem do commit:
```bash
git commit -m "docs: fix typo [skip-ci]"
```

## Fluxo Completo

### 1. Desenvolvimento
```bash
# Fazer mudanças
git add .
git commit -m "feat: add new caching algorithm"
git push origin main
```

### 2. Automação (GitHub Actions)
1. **CI Workflow**: Testa e builda
2. **Docker Deploy**: Publica imagem
3. **Version Workflow**: Analisa commits e incrementa versão
4. **Release**: Cria tag e release

### 3. Resultado
- Nova versão no `Cargo.toml`
- Tag `v0.1.0` criada
- Release no GitHub com changelog
- Imagem Docker `usuario/crabcache:v0.1.0`

## Estrutura de Tags

### Automáticas
- `latest`: Sempre aponta para main
- `v1.2.3`: Versão específica do release
- `main-abc1234`: Build específico da main

### Manuais
- `v1.2.3-rc.1`: Release candidate
- `v1.2.3-beta.1`: Versão beta

## Changelog Automático

O sistema gera changelog categorizado:

```markdown
## [0.1.0] - 2024-01-15

### 💥 BREAKING CHANGES
- Change API response format

### ✨ Features
- Add Redis protocol compatibility
- Implement LRU eviction policy

### 🐛 Bug Fixes
- Resolve race condition in concurrent access
- Prevent memory leak in cleanup process

### ⚡ Performance
- Optimize hash table operations
- Reduce memory allocation overhead

### 📚 Documentation
- Update installation guide
- Add API examples
```

## Troubleshooting

### Versão não incrementou
- Verifique se o commit segue conventional commits
- Confirme que não há `[skip-version]` na mensagem
- Verifique se há commits novos desde a última tag

### Erro de permissão
- Confirme que `GITHUB_TOKEN` tem permissões adequadas
- Verifique se o repositório permite Actions

### Build falhou
- Verifique os logs na aba Actions
- Confirme que todos os testes passam
- Verifique se o `Cargo.toml` está válido

## Configuração Inicial

1. **Workflows já configurados** ✅
2. **Configure secrets Docker** (se ainda não fez):
   - `DOCKER_USERNAME`
   - `DOCKER_PASSWORD`
3. **Teste o sistema**:
   ```bash
   git commit -m "feat: test automatic versioning"
   git push origin main
   ```

## Boas Práticas

### Commits
- Use conventional commits sempre
- Seja descritivo nas mensagens
- Agrupe mudanças relacionadas

### Releases
- Teste localmente antes do push
- Revise o changelog gerado
- Documente breaking changes

### Versionamento
- Use `patch` para correções
- Use `minor` para novas features
- Use `major` apenas para breaking changes
- Considere usar pre-releases para testes