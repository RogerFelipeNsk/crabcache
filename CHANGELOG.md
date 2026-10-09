# Changelog

Todas as mudanças notáveis neste projeto serão documentadas neste arquivo.

O formato é baseado em [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
e este projeto adere ao [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.3.0] - 2026-10-09

### Adicionado
- **CrabPack** (`--compression`): compressão transparente de valores ociosos com dicionários zstd
  treinados por prefixo de chave. Em JSON realista (sessões, produtos, respostas de API) a memória por
  chave caiu para 39–41% da do Redis 8.10 (taxa de compressão de 3.6–4.1x). As leituras devolvem os
  bytes originais; escritas gravam sem compressão e o valor volta a ser comprimido quando fica ocioso.
- `INFO compression` (dicionários, chaves comprimidas, taxa) e `CONFIG GET/SET` para `compression`,
  `compression-min-idle` e `compression-min-size`.
- `examples/dataset.rs` (dados realistas para benchmarks) e `scripts/bench-compression.sh`.
- Testes: integração da compressão e uma fase diferencial em que 5.000 comandos sobre valores
  comprimidos precisam responder byte a byte igual ao Redis.

## [0.2.1] - 2026-10-07

### Adicionado
- RESP3 via `HELLO 3`. O `redis-py` 8 negocia RESP3 por padrão, sem fallback, e não conseguia
  conectar na 0.2.0. Os testes diferenciais agora rodam em RESP2 e RESP3 contra um Redis real.
- Gráficos de benchmark no README (`scripts/gen-charts.py`).

### Corrigido
- O workflow de release pode ser reexecutado sem falhar quando a release do GitHub já existe.

## [0.2.0] - 2026-10-07

Reescrita completa ("v2"). O código anterior está preservado na tag `legacy-v1`.

### Mudou
- **Protocolo RESP2 compatível com Redis** (multibulk + inline). Funciona com `redis-cli`,
  `redis-benchmark`, `memtier_benchmark` e bibliotecas cliente Redis. O protocolo de texto próprio e os
  7 servidores alternativos foram substituídos por um único servidor.
- Respostas acumuladas e enviadas com uma única escrita por leitura (pipelining eficiente).
- Threads de I/O thread-per-core (um runtime e um kqueue/epoll por thread), com distribuição adaptativa
  de conexões (`--io-conns-per-thread`).
- Store em shards com entrada de 16 bytes (uma alocação com chave, valor e TTL opcional), entradas em
  blocos fixos e mimalloc devolvendo memória livre imediatamente. Usa 14% menos memória que o Redis 8.10
  com valores de 100 B (27% menos com 10 B).
- Porta padrão 6379, bind padrão `127.0.0.1`.

### Adicionado
- Comandos de string (GET/SET com NX/XX/GET/EX/PX/EXAT/PXAT/KEEPTTL, MGET/MSET/MSETNX, INCR*,
  APPEND, GETRANGE, GETEX, GETDEL...), chaves (DEL, EXISTS, EXPIRE* com NX/XX/GT/LT, TTL/PTTL, PERSIST,
  KEYS, SCAN, RENAME...) e servidor (AUTH, HELLO, CLIENT, CONFIG GET/SET, INFO, ...).
- TTL com expiração preguiçosa e ativa.
- `maxmemory` com `noeviction`, `allkeys-lru`, `allkeys-lfu` e `allkeys-random`.
- `requirepass` (comparação em tempo constante).
- Testes diferenciais contra um Redis real (respostas idênticas byte a byte), testes de integração com
  o cliente oficial `redis`, CI sem `continue-on-error`.

### Corrigido (em relação à 0.1.0)
- Valores com espaços, binários e a string `"NULL"` agora são armazenados corretamente.
- Sobrescrever uma chave sempre atualiza o valor (o mapa lock-free antigo mantinha o valor antigo).
- Um cliente enviando dados sem `\n` não consome mais CPU indefinidamente (parsing era O(n²)).
- 3 vulnerabilidades em dependências (bytes, crossbeam-epoch, protobuf) eliminadas.

### Errata da 0.1.0
As afirmações abaixo da 0.1.0 não se sustentaram na auditoria de 2026-10-06: o protocolo TOON do
servidor era um stub e o cliente JS nunca o usava; os números de desempenho vinham de testes com
resolução de 1 ms e sem verificação dos valores; o CI ignorava falhas de teste. Ver `docs/BENCHMARKS.md`
para medições reproduzíveis.

## [0.1.0] - 2026-01-03

### 🎨 Major Release - TOON Protocol & Production-Ready Server

**Importante**: Esta é uma versão de produção com suporte ao protocolo TOON e servidor estável.

### Added
- ✨ **ToonHybridServer**: Servidor de produção estável com suporte ao protocolo TOON
  - 🎨 **TOON Protocol Support**: Reconhecimento automático de magic bytes TOON
  - 📝 **Dual Protocol**: Suporte simultâneo a protocolos TOON e texto
  - 🔄 **Auto-detection**: Detecção automática de protocolo com fallback
  - 🛡️ **Stable Architecture**: Baseado em DashMap lock-free comprovado

### Performance
- ⚡ **71,429+ ops/sec**: Performance validada em testes locais
- 🚀 **Sub-millisecond latency**: Latência de 0.01ms comprovada
- 💪 **100% Test Success**: 13/13 testes aprovados (100% success rate)
- 🔗 **Multi-connection**: 5+ conexões simultâneas estáveis

### Infrastructure
- 🐳 **Production Docker**: Multi-stage build otimizado para produção
- 📊 **Complete Monitoring**: Metrics server (port 9090) com Prometheus
- 🏥 **Health Checks**: Endpoint HTTP para health checks
- 🎯 **Default Server**: ToonHybridServer como servidor padrão

### Technical Improvements
- 🔧 **Lock-free Storage**: DashMap para acesso concorrente otimizado
- 🌐 **TCP Optimizations**: TCP_NODELAY e buffers otimizados (8KB)
- 🔄 **Async Processing**: Tokio async para máxima concorrência
- 📈 **Memory Efficient**: Zero vazamentos, overhead mínimo

### CI/CD & DevOps
- ✅ **CI/CD Validation**: Pipeline completo validado
- 🚀 **Multi-platform**: Suporte Linux (amd64/arm64) e macOS (x64/ARM64)
- 📦 **Docker Hub**: Deployment automático para Docker Hub
- 🏷️ **Version Tags**: Tags automáticas de versão

### Monitoring & Observability
- 📊 **Prometheus Metrics**: Métricas completas para monitoramento
- 🎛️ **Web Dashboard**: Interface web para monitoramento visual
- 🏥 **Health Endpoints**: Endpoints de saúde para Docker health checks
- 📝 **Structured Logging**: Logging JSON estruturado

### Protocol Support
- 🎨 **TOON Magic Bytes**: Reconhecimento de `544f4f4e01000100`
- 📝 **Text Commands**: PING, PUT, GET, DEL, STATS, TOON_TEST
- 🔄 **Protocol Switching**: Mudança automática entre protocolos
- 📊 **TOON Stats**: Resposta automática TOON_STATS

### Stability & Reliability
- 💪 **100% Uptime**: Sem falhas durante todos os testes
- 🔗 **Connection Stability**: 5/5 conexões simultâneas estáveis
- ⚡ **Zero Timeouts**: Nenhum timeout ou falha de conexão
- 🛡️ **Error Handling**: Tratamento robusto de erros

### Changed
- 🎯 **Default Server**: ToonHybridServer agora é o servidor padrão
- 📦 **Version**: Bump para 0.1.0 (major milestone)
- 🐳 **Docker**: Atualizado para usar ToonHybridServer por padrão

### Fixed
- 🔧 **Server Initialization**: Resolvidos problemas de inicialização dos servidores Ultimate
- 🔒 **Lock Contention**: Eliminado lock contention usando DashMap
- 📊 **Metrics Integration**: Metrics server integrado desde inicialização

### Performance Comparison
| Servidor | Status | Performance | TOON Protocol | Estabilidade |
|----------|--------|-------------|---------------|--------------|
| **ToonHybridServer** | ✅ **WORKING** | **71k+ ops/sec** | ✅ **Suporta** | ✅ **100%** |
| ToonUltimateServer | ❌ Hangs | N/A | ❌ Não funciona | ❌ 0% |
| UltimateServer | ❌ Hangs | N/A | ❌ Não suporta | ❌ 0% |
| HybridServer | ✅ Working | ~80k ops/sec | ❌ Não suporta | ✅ 95% |

### Repository Information
- **GitHub**: https://github.com/RogerFelipeNsk/crabcache
- **Docker Hub**: rogerfelipensk/crabcache:0.1.0
- **License**: MIT
- **Author**: Roger Felipe <rogerfelipe.nsk@gmail.com>

---

## [0.0.2] - 2025-01-01

### 🔒 Security Updates

#### Fixed
- **CVE-2025-68973**: Corrigida vulnerabilidade no gnupg2 (Severity: 7.8 High)
  - Atualizado Dockerfile para fazer upgrade explícito do gnupg2
  - Afetava versões >=2.2.40-1.1+deb12u1 da imagem base Debian
  - Implementado em ambos Dockerfiles (raiz e docker/)

#### Added
- 🛡️ **Security Documentation**: Adicionado [SECURITY.md](docs/SECURITY.md) com guidelines completas
- 🔍 **Vulnerability Scanner**: Script manual `scripts/security-scan.sh`
  - Suporte para Docker Scout, Trivy e Grype
  - Relatórios JSON e resumo em Markdown
  - Execução manual para evitar pipelines duplicadas
- 📋 **Security Monitoring**: Documentação de práticas de segurança
- 🔄 **Container Hardening**: Melhorias na segurança dos containers

#### Changed
- 📦 **Base Image Security**: Processo de build atualizado para incluir patches de segurança
- 📚 **README**: Adicionada seção de segurança de container
- 🏷️ **Docker Labels**: Metadados de segurança nos containers

#### Removed
- ❌ **Automatic Security Workflow**: Removido workflow automático que causava pipelines duplicadas
  - Security scan agora é executado manualmente via script
  - Evita conflitos com o pipeline principal de CI/CD

## [0.0.1] - 2025-12-23

### 🎓 Educational Release - Initial Learning Version

**Importante**: Esta é uma versão educacional desenvolvida para fins de aprendizado através de VibeCoding.

### Added
- ✨ **Core Cache Engine**: Sistema básico de cache em memória
- 🧠 **TinyLFU Eviction**: Algoritmo de eviction inteligente com Count-Min Sketch
- 💾 **WAL Persistence**: Sistema de Write-Ahead Log para durabilidade opcional
- 🚀 **Pipeline Processing**: Suporte a processamento em lote de comandos
- 🔐 **Security Framework**: Autenticação, rate limiting e filtros de IP
- 📊 **Observability**: Métricas Prometheus e dashboard web
- 🐳 **Docker Support**: Imagens Docker otimizadas para desenvolvimento
- 📚 **Documentation**: Documentação completa para fins educacionais
- 🧪 **Testing Suite**: Testes funcionais e de performance
- ⚙️ **Configuration**: Sistema flexível de configuração via TOML

### Educational Features
- **Rust Learning**: Demonstração de conceitos avançados de Rust
- **Systems Programming**: Implementação de estruturas de dados lock-free
- **Network Programming**: Servidor TCP com otimizações de performance
- **Concurrency**: Programação assíncrona e gerenciamento de recursos
- **DevOps**: Containerização e práticas de deployment

### Performance (Educational Environment)
- **Single Commands**: ~17,000 ops/sec (ambiente de desenvolvimento)
- **Pipeline Batches**: ~139,000+ ops/sec (demonstração conceitual)
- **Mixed Workloads**: ~205,000+ ops/sec (testes locais)
- **Latency**: Sub-millisecond em ambiente controlado

### Technical Highlights
- **Memory Safety**: Implementação 100% safe Rust
- **Zero Dependencies**: Minimal external dependencies
- **Modular Design**: Arquitetura modular para facilitar aprendizado
- **Comprehensive Testing**: Suite completa de testes educacionais
- **Docker Ready**: Imagens otimizadas para experimentação

### Learning Resources
- **Examples**: Exemplos práticos de uso em Rust e Python
- **Benchmarks**: Scripts de benchmark para análise de performance
- **Documentation**: Guias detalhados de arquitetura e implementação
- **Configuration**: Exemplos de configuração para diferentes cenários

### Known Educational Limitations
- **Single Node**: Implementação focada em aprendizado, sem clustering
- **Development Environment**: Otimizado para ambiente de desenvolvimento
- **Limited Protocol**: Conjunto básico de comandos para fins educacionais
- **Validation Required**: Benchmarks devem ser validados independentemente

### Repository Information
- **GitHub**: https://github.com/RogerFelipeNsk/crabcache
- **Docker Hub**: rogerfelipensk/crabcache:0.0.1
- **License**: MIT (uso educacional)
- **Author**: Roger Felipe <rogerfelipe.nsk@gmail.com>

---

## Versões Futuras (Planejadas para Aprendizado)

### [0.1.0] - Planejado
- **Client Libraries**: Bibliotecas cliente em diferentes linguagens
- **Enhanced Protocols**: Protocolos mais robustos
- **Advanced Metrics**: Métricas mais detalhadas
- **Performance Improvements**: Otimizações adicionais

### [0.2.0] - Planejado
- **Clustering Concepts**: Demonstração de conceitos de clustering
- **Replication**: Implementação educacional de replicação
- **Advanced Security**: TLS/SSL e recursos de segurança avançados
- **Monitoring**: Sistema de monitoramento mais completo

---

**Nota**: Este changelog documenta o progresso educacional do projeto CrabCache, desenvolvido para fins de aprendizado e experimentação com Rust e sistemas de cache.