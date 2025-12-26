# 🛠️ CrabCache Scripts Essenciais

Esta pasta contém apenas os scripts essenciais para o funcionamento, teste e deploy do CrabCache.

## 📋 Scripts Disponíveis

### 🧪 Testes

#### `final_system_test.py`
**Teste completo e abrangente do sistema**

```bash
# Teste completo com todas as funcionalidades
python3 scripts/final_system_test.py

# Teste com servidor remoto
python3 scripts/final_system_test.py --host 192.168.1.100 --port 8000

# Salvar resultados em arquivo
python3 scripts/final_system_test.py --output results.json
```

**Funcionalidades testadas:**
- ✅ Operações básicas (PUT/GET/DEL/TTL)
- ✅ Pipeline processing com performance
- ✅ Mixed workload (operações mistas)
- ✅ Stress testing com lotes grandes
- ✅ Estatísticas do sistema

#### `test_simple.py`
**Teste rápido e básico para validação**

```bash
# Teste básico (inicia container automaticamente se necessário)
python3 scripts/test_simple.py
```

**Funcionalidades testadas:**
- ✅ Conectividade básica
- ✅ Operações CRUD simples
- ✅ Endpoints de métricas
- ✅ Performance básica

### 🐳 Docker e Deploy

#### `docker_build_and_publish.sh`
**Build e publicação de imagens Docker**

```bash
# Build completo com testes e publicação
./scripts/docker_build_and_publish.sh

# Build sem testes
./scripts/docker_build_and_publish.sh --skip-tests

# Build sem publicação (apenas local)
./scripts/docker_build_and_publish.sh --skip-push

# Build com versão específica
./scripts/docker_build_and_publish.sh --version 1.0.0
```

**Funcionalidades:**
- 🔨 Build da imagem Docker
- 🧪 Teste da imagem construída
- 📤 Publicação no Docker Hub
- 📊 Informações detalhadas da imagem

### ✅ Validação

#### `validate-ci-locally.sh`
**Validação local dos comandos do CI**

```bash
# Executar validação completa (simula CI)
./scripts/validate-ci-locally.sh
```

**Validações executadas:**
- 🎨 Formatação do código (cargo fmt)
- 🔍 Análise estática (cargo clippy)
- 🧪 Testes unitários (com timeouts)
- 🔨 Build release
- 🐳 Build e teste Docker

### 📦 Utilitários

#### `version.sh`
**Controle de versão do projeto**

```bash
# Mostrar versão atual
./scripts/version.sh

# Definir nova versão
./scripts/version.sh 1.0.0
```

## 🚀 Fluxo de Desenvolvimento Recomendado

### 1. Desenvolvimento Local
```bash
# 1. Validar mudanças localmente
./scripts/validate-ci-locally.sh

# 2. Teste rápido
python3 scripts/test_simple.py

# 3. Teste completo
python3 scripts/final_system_test.py
```

### 2. Preparação para Release
```bash
# 1. Atualizar versão
./scripts/version.sh 1.0.0

# 2. Build e publicação
./scripts/docker_build_and_publish.sh

# 3. Validação final
python3 scripts/final_system_test.py --host localhost --port 8000
```

## 📊 Comparação: Antes vs Depois

### ❌ Antes da Limpeza
- **58 scripts** (muitos redundantes)
- **Confusão** sobre qual script usar
- **Manutenção complexa** com muitos arquivos
- **Benchmarks duplicados** e específicos

### ✅ Depois da Limpeza
- **5 scripts essenciais** + README
- **Propósito claro** para cada script
- **Manutenção simples** e focada
- **Funcionalidade completa** mantida

## 🎯 Casos de Uso

### Para Desenvolvedores
- **Desenvolvimento**: `test_simple.py` para validação rápida
- **CI/CD**: `validate-ci-locally.sh` antes de push
- **Release**: `docker_build_and_publish.sh` para deploy

### Para QA/Testes
- **Validação completa**: `final_system_test.py`
- **Testes de regressão**: `test_simple.py`
- **Performance**: Métricas incluídas nos testes

### Para DevOps
- **Deploy**: `docker_build_and_publish.sh`
- **Versionamento**: `version.sh`
- **Validação**: `validate-ci-locally.sh`

## 🔧 Requisitos

### Python Scripts
- Python 3.8+
- Bibliotecas: `socket`, `time`, `json` (padrão)
- Opcional: `requests` para testes de endpoints HTTP

### Shell Scripts
- Bash 4.0+
- Docker (para scripts de build)
- Cargo/Rust (para validação)

## 📝 Notas

- **Todos os scripts são independentes** e podem ser executados isoladamente
- **Logs detalhados** com códigos de cores para melhor visualização
- **Tratamento de erros** robusto com códigos de saída apropriados
- **Documentação inline** em cada script para referência

---

**Scripts Essenciais v1.0** - Simplicidade e funcionalidade completa 🛠️🦀