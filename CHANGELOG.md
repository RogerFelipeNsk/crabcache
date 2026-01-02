# Changelog

Todas as mudanças notáveis neste projeto serão documentadas neste arquivo.

O formato é baseado em [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
e este projeto adere ao [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.0.2] - 2025-01-01

### 🔒 Security Updates

#### Fixed
- **CVE-2025-68973**: Corrigida vulnerabilidade no gnupg2 (Severity: 7.8 High)
  - Atualizado Dockerfile para fazer upgrade explícito do gnupg2
  - Afetava versões >=2.2.40-1.1+deb12u1 da imagem base Debian
  - Implementado em ambos Dockerfiles (raiz e docker/)

#### Added
- 🛡️ **Security Documentation**: Adicionado [SECURITY.md](docs/SECURITY.md) com guidelines completas
- 🔍 **Vulnerability Scanner**: Script automatizado `scripts/security-scan.sh`
  - Suporte para Docker Scout, Trivy e Grype
  - Relatórios JSON e resumo em Markdown
  - Execução automatizada de múltiplas ferramentas
- 📋 **Security Monitoring**: Documentação de práticas de segurança
- 🔄 **Container Hardening**: Melhorias na segurança dos containers

#### Changed
- 📦 **Base Image Security**: Processo de build atualizado para incluir patches de segurança
- 📚 **README**: Adicionada seção de segurança de container
- 🏷️ **Docker Labels**: Metadados de segurança nos containers

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