# 🦀 CrabCache

<div align="center">
  <img src="assets/logo.png" alt="CrabCache Logo" width="200" height="200">
  
  [![Rust](https://img.shields.io/badge/rust-1.92+-orange.svg)](https://www.rust-lang.org)
  [![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
  [![Version](https://img.shields.io/badge/version-0.0.2-green.svg)](#version)
  [![Docker](https://img.shields.io/badge/docker-ready-blue.svg)](Dockerfile)
</div>

**CrabCache** é um sistema de cache em memória de alta performance escrito em Rust, projetado para aplicações que exigem baixa latência e alto throughput. Oferece compatibilidade com protocolo Redis e recursos avançados como eviction inteligente, persistência opcional e monitoramento integrado.

## 🚀 Características Principais

### ⚡ Performance
- **Alta performance**: Otimizado para baixa latência e alto throughput
- **Multi-threading**: Suporte nativo a processamento paralelo
- **Lock-free**: Estruturas de dados sem bloqueios para máxima concorrência
- **SIMD**: Otimizações vetorizadas quando disponíveis

### 🧠 Eviction Inteligente
- **Algoritmo TinyLFU** com Count-Min Sketch otimizado
- **Estratégias configuráveis**: Gradual ou batch eviction
- **Window LRU** para itens recentemente inseridos
- **Memory pressure monitoring** automático
- **Adaptive eviction** baseado na pressão de memória

### 💾 Persistência Opcional
- **Write-Ahead Log (WAL)** segmentado
- **Recovery automático** em caso de falhas
- **Políticas de sync** configuráveis (None/Async/Sync)
- **Integridade de dados** com checksums CRC32

### 🔐 Segurança
- **Autenticação por token** com múltiplos tokens
- **Rate limiting** com algoritmo token bucket
- **IP filtering** com suporte CIDR (IPv4/IPv6)
- **Connection limits** configuráveis
- **Container security**: Imagens atualizadas contra CVEs conhecidas
- **Vulnerability scanning**: Scripts automatizados para detecção de vulnerabilidades

### 📊 Observabilidade
- **Métricas Prometheus** nativas
- **Dashboard web** em tempo real
- **Health checks** integrados
- **Logs estruturados** JSON
- **Histogramas de latência** precisos

## �️ Insotalação

### Via Docker (Recomendado)

```bash
# Executar com configuração padrão
docker run -p 8000:8000 -p 9090:9090 crabcache:latest

# Com persistência WAL
docker run -p 8000:8000 -p 9090:9090 \
  -e CRABCACHE_ENABLE_WAL=true \
  -e CRABCACHE_WAL_SYNC_POLICY=async \
  -v /data/wal:/app/data/wal \
  crabcache:latest
```

### Build do Código Fonte

```bash
# Clone o repositório
git clone https://github.com/your-org/crabcache.git
cd crabcache

# Build release
cargo build --release

# Executar
./target/release/crabcache
```

## 🛡️ Segurança de Container

### Verificação de Vulnerabilidades

O CrabCache inclui ferramentas para verificar vulnerabilidades de segurança:

```bash
# Executar scan de segurança automatizado
./scripts/security-scan.sh

# Scan de imagem específica
./scripts/security-scan.sh -i crabcache:v0.0.2

# Resultados salvos em ./security-reports/
```

### CVEs Resolvidas

- **CVE-2025-68973**: Vulnerabilidade gnupg2 corrigida (Dezembro 2024)
  - Severity: 7.8 (High)
  - Status: ✅ Resolvida via atualização de pacote

### Práticas de Segurança

- 🔄 **Atualizações regulares**: Base images atualizadas automaticamente
- 🔒 **Usuário não-root**: Container executa como usuário dedicado
- 📊 **Scanning contínuo**: Verificação automática de vulnerabilidades
- 📋 **Documentação**: Veja [SECURITY.md](docs/SECURITY.md) para detalhes completos

## 🔧 Configuração

### Arquivo TOML

```toml
# config/default.toml
bind_addr = "0.0.0.0"
port = 8000
max_memory_per_shard = 1073741824  # 1GB

[security]
enable_auth = false
enable_tls = false
allowed_ips = []
max_command_size = 1048576

[rate_limiting]
enabled = false
max_requests_per_second = 1000
burst_capacity = 100

[eviction]
enabled = true
window_ratio = 0.01
memory_high_watermark = 0.85
memory_low_watermark = 0.70
eviction_strategy = "batch"
batch_eviction_size = 50
adaptive_eviction = true

[wal]
max_segment_size = 67108864  # 64MB
sync_policy = "async"
```

### Variáveis de Ambiente

```bash
# Servidor
CRABCACHE_PORT=8000
CRABCACHE_BIND_ADDR=0.0.0.0

# Segurança
CRABCACHE_ENABLE_AUTH=true
CRABCACHE_AUTH_TOKEN=your-secret-token
CRABCACHE_ALLOWED_IPS=127.0.0.1,192.168.1.0/24

# Rate Limiting
CRABCACHE_ENABLE_RATE_LIMIT=true
CRABCACHE_MAX_REQUESTS_PER_SECOND=1000

# WAL Persistência
CRABCACHE_ENABLE_WAL=true
CRABCACHE_WAL_SYNC_POLICY=async
CRABCACHE_WAL_DIR=./data/wal

# Eviction
CRABCACHE_EVICTION_ENABLED=true
CRABCACHE_EVICTION_STRATEGY=batch
CRABCACHE_EVICTION_HIGH_WATERMARK=0.85
CRABCACHE_EVICTION_LOW_WATERMARK=0.70

# Logging
CRABCACHE_LOG_LEVEL=info
CRABCACHE_LOG_FORMAT=json
```

## 🔌 Uso

### Protocolo de Texto

```bash
# Conectar via telnet/nc
nc localhost 8000

# Comandos básicos
PING                    # Resposta: PONG
PUT key value          # Resposta: OK
GET key                # Resposta: value
DEL key                # Resposta: OK
EXPIRE key 60          # Resposta: OK
STATS                  # Resposta: JSON com métricas
```

### Cliente Python

```python
import socket

# Conectar
sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
sock.connect(('localhost', 8000))

# Enviar comandos
sock.send(b'PUT user:123 {"name":"Alice"}\n')
response = sock.recv(4096)  # b'OK\n'

sock.send(b'GET user:123\n')
response = sock.recv(4096)  # b'{"name":"Alice"}\n'

sock.close()
```

## 📊 Monitoramento

### Métricas Prometheus

```bash
# Acessar métricas
curl http://localhost:9090/metrics

# Principais métricas
crabcache_operations_total
crabcache_latency_histogram
crabcache_memory_usage_bytes
crabcache_evictions_total
crabcache_connections_active
```

### Dashboard Web

Acesse `http://localhost:9090/dashboard` para ver:

- **Performance em tempo real**: Throughput e latência
- **Uso de memória**: Por shard e total
- **Taxa de hit/miss**: Eficiência do cache
- **Métricas de eviction**: Algoritmo TinyLFU
- **Status de conexões**: Conexões ativas e rate limiting

### Health Check

```bash
curl http://localhost:9090/health
# {"status":"healthy","service":"crabcache","version":"0.0.2"}
```

## 🏗️ Arquitetura

### Componentes Principais

```
┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐
│   TCP Server    │    │  Security Mgr   │    │  Metrics Mgr    │
│                 │    │                 │    │                 │
│ • Connection    │    │ • Authentication│    │ • Prometheus    │
│ • Protocol      │    │ • Rate Limiting │    │ • Dashboard     │
│ • Routing       │    │ • IP Filtering  │    │ • Health Check  │
└─────────────────┘    └─────────────────┘    └─────────────────┘
         │                       │                       │
         └───────────────────────┼───────────────────────┘
                                 │
                    ┌─────────────────┐
                    │  Shard Router   │
                    │                 │
                    │ • Hash-based    │
                    │ • Load Balance  │
                    │ • Fault Tolerant│
                    └─────────────────┘
                                 │
         ┌───────────────────────┼───────────────────────┐
         │                       │                       │
┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐
│    Shard 0      │    │    Shard 1      │    │    Shard N      │
│                 │    │                 │    │                 │
│ • TinyLFU       │    │ • TinyLFU       │    │ • TinyLFU       │
│ • WAL Writer    │    │ • WAL Writer    │    │ • WAL Writer    │
│ • Lock-free Map │    │ • Lock-free Map │    │ • Lock-free Map │
│ • TTL Wheel     │    │ • TTL Wheel     │    │ • TTL Wheel     │
└─────────────────┘    └─────────────────┘    └─────────────────┘
```

### Fluxo de Dados

1. **Conexão**: Cliente conecta via TCP
2. **Autenticação**: Verificação de token (se habilitada)
3. **Rate Limiting**: Controle de taxa de requisições
4. **Parsing**: Comando parseado e validado
5. **Routing**: Comando roteado para shard apropriado
6. **Execução**: Operação executada no shard
7. **Eviction**: TinyLFU decide evictions se necessário
8. **WAL**: Operação logada para persistência (opcional)
9. **Resposta**: Resultado enviado ao cliente
10. **Métricas**: Estatísticas atualizadas

## 👨‍💻 Desenvolvimento

### Comandos Básicos

```bash
# Formatação
cargo fmt

# Build
cargo build --release

# Testes
cargo test

# Linting
cargo clippy

# Documentação
cargo doc --open
```

### Estrutura do Projeto

```
crabcache/
├── src/                    # Código fonte
│   ├── config/            # Sistema de configuração
│   ├── eviction/          # Algoritmos de eviction
│   ├── metrics/           # Sistema de métricas
│   ├── protocol/          # Protocolos de comunicação
│   ├── security/          # Sistema de segurança
│   ├── server/            # Servidor TCP
│   ├── shard/             # Sistema de sharding
│   ├── store/             # Armazenamento lock-free
│   └── wal/               # Write-Ahead Log
├── examples/              # Exemplos de uso
├── docs/                  # Documentação
├── config/                # Arquivos de configuração
└── proto/                 # Schemas Protobuf
```

## 📚 Documentação

### Documentação Principal
- **[API Reference](docs/API.md)** - Documentação completa da API
- **[Sistema de Eviction](docs/EVICTION_SYSTEM.md)** - Algoritmo TinyLFU
- **[Persistência WAL](docs/WAL_PERSISTENCE.md)** - Write-Ahead Log
- **[Sistema de Segurança](docs/SECURITY_SYSTEM.md)** - Autenticação e controle
- **[Análise de Performance](docs/PERFORMANCE_ANALYSIS.md)** - Benchmarks e otimizações
- **[Guia de Contribuição](docs/CONTRIBUTING.md)** - Como contribuir

## 🤝 Contribuindo

### Guidelines

1. **Código**: Siga as convenções Rust (rustfmt, clippy)
2. **Testes**: Adicione testes para novas funcionalidades
3. **Documentação**: Documente APIs públicas
4. **Performance**: Mantenha benchmarks atualizados
5. **Segurança**: Considere implicações de segurança

### Fluxo de Trabalho

```bash
# 1. Fork e clone
git clone https://github.com/your-fork/crabcache.git
cd crabcache

# 2. Criar branch
git checkout -b feature/nova-funcionalidade

# 3. Desenvolver e testar
cargo test
cargo clippy

# 4. Commit e push
git commit -m "feat: nova funcionalidade"
git push origin feature/nova-funcionalidade

# 5. Abrir Pull Request
```

## 📄 Licença

Este projeto está licenciado sob a licença MIT - veja o arquivo [LICENSE](LICENSE) para detalhes.

## 🙏 Agradecimentos

- **Rust Community**: Pela linguagem incrível
- **Redis**: Pela inspiração e referência
- **TinyLFU Paper**: Pelo algoritmo de eviction
- **Tokio**: Pelo runtime async excepcional

## 📞 Suporte

- **Issues**: [GitHub Issues](https://github.com/your-org/crabcache/issues)
- **Discussions**: [GitHub Discussions](https://github.com/your-org/crabcache/discussions)

---

**CrabCache** - *Sistema de cache em memória de alta performance escrito em Rust* 🦀⚡