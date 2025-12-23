# Contribuindo para o CrabCache

Obrigado pelo seu interesse em contribuir para o CrabCache! Este documento fornece diretrizes para contribuições.

## 🚀 Como Contribuir

### 1. Setup do Ambiente

```bash
# Clone o repositório
git clone https://github.com/your-org/crabcache.git
cd crabcache

# Instale Rust (se não tiver)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Build do projeto
cargo build

# Execute os testes
cargo test

# Execute todos os testes
python3 scripts/run_all_tests.py
```

### 2. Estrutura do Projeto

```
crabcache/
├── src/                    # Código fonte principal
│   ├── client/            # Cliente nativo Rust
│   ├── config/            # Sistema de configuração
│   ├── eviction/          # Algoritmos de eviction (TinyLFU)
│   ├── metrics/           # Sistema de métricas e observabilidade
│   ├── protocol/          # Protocolos de comunicação
│   ├── security/          # Sistema de segurança
│   ├── server/            # Servidor TCP
│   ├── shard/             # Gerenciamento de shards
│   ├── store/             # Estruturas de dados core
│   ├── ttl/               # Sistema de TTL
│   ├── wal/               # Write-Ahead Log
│   └── utils/             # Utilitários
├── config/                # Arquivos de configuração
├── docs/                  # Documentação
├── examples/              # Exemplos de uso
├── scripts/               # Scripts de teste e benchmark
├── benches/               # Benchmarks
├── tests/                 # Testes de integração
└── docker/                # Dockerfiles
```

### 3. Tipos de Contribuição

#### 🐛 Bug Reports
- Use o template de issue para bugs
- Inclua informações do sistema
- Forneça passos para reproduzir
- Inclua logs relevantes

#### ✨ Feature Requests
- Descreva o caso de uso
- Explique o benefício
- Considere alternativas
- Discuta impacto na performance

#### 🔧 Code Contributions
- Fork o repositório
- Crie uma branch para sua feature
- Siga as convenções de código
- Adicione testes
- Atualize documentação

### 4. Convenções de Código

#### Rust Style
```rust
// Use rustfmt
cargo fmt

// Use clippy
cargo clippy -- -D warnings

// Documente APIs públicas
/// Calculates the hash for a given key
pub fn hash_key(key: &str) -> u64 {
    // Implementation
}

// Use Result para error handling
pub fn risky_operation() -> Result<String, Error> {
    // Implementation
}
```

#### Naming Conventions
- **Structs**: `PascalCase` (ex: `TcpServer`)
- **Functions**: `snake_case` (ex: `process_command`)
- **Constants**: `SCREAMING_SNAKE_CASE` (ex: `MAX_CONNECTIONS`)
- **Modules**: `snake_case` (ex: `rate_limit`)

#### Performance Guidelines
- Prefira `&str` sobre `String` quando possível
- Use `Arc` para dados compartilhados
- Evite clones desnecessários
- Considere zero-copy operations
- Profile código crítico

### 5. Testes

#### Testes Unitários
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_functionality() {
        // Test implementation
    }

    #[tokio::test]
    async fn test_async_functionality() {
        // Async test implementation
    }
}
```

#### Testes de Integração
```bash
# Teste básico
python3 scripts/test_simple.py

# Teste WAL
python3 scripts/test_wal_focused.py

# Teste de segurança
python3 scripts/test_security.py

# Todos os testes
python3 scripts/run_all_tests.py
```

#### Benchmarks
```bash
# Benchmark interno
cargo bench

# Benchmark completo
python3 scripts/benchmark_complete.py
```

### 6. Documentação

#### Código
- Documente todas as APIs públicas
- Use exemplos em doc comments
- Explique algoritmos complexos
- Documente invariantes importantes

#### Markdown
- Use títulos hierárquicos
- Inclua exemplos de código
- Adicione diagramas quando útil
- Mantenha TOC atualizado

### 7. Pull Request Process

#### Antes de Submeter
1. Execute todos os testes
2. Execute `cargo fmt`
3. Execute `cargo clippy`
4. Atualize documentação
5. Adicione entrada no CHANGELOG

#### PR Template
```markdown
## Descrição
Breve descrição das mudanças.

## Tipo de Mudança
- [ ] Bug fix
- [ ] Nova feature
- [ ] Breaking change
- [ ] Documentação

## Testes
- [ ] Testes unitários passando
- [ ] Testes de integração passando
- [ ] Benchmarks executados

## Checklist
- [ ] Código formatado (rustfmt)
- [ ] Linting passou (clippy)
- [ ] Documentação atualizada
- [ ] Testes adicionados/atualizados
```

### 8. Performance Guidelines

#### Otimizações Críticas
- **Zero-copy**: Evite cópias desnecessárias
- **SIMD**: Use instruções vetoriais quando possível
- **Lock-free**: Prefira estruturas lock-free
- **Memory pools**: Reutilize buffers
- **Async**: Use async/await para I/O

#### Profiling
```bash
# Profile com perf
cargo build --release
perf record --call-graph=dwarf ./target/release/crabcache
perf report

# Profile com flamegraph
cargo install flamegraph
cargo flamegraph --bin crabcache
```

### 9. Segurança

#### Security Guidelines
- Valide todas as entradas
- Use bibliotecas criptográficas estabelecidas
- Evite timing attacks
- Sanitize logs (não logue tokens)
- Considere DoS attacks

#### Reporting Security Issues
- **NÃO** abra issues públicas para vulnerabilidades
- Envie email para: security@crabcache.io
- Inclua detalhes da vulnerabilidade
- Aguarde resposta antes de disclosure público

### 10. Release Process

#### Versioning
Seguimos [Semantic Versioning](https://semver.org/):
- **MAJOR**: Breaking changes
- **MINOR**: New features (backward compatible)
- **PATCH**: Bug fixes

#### Release Checklist
1. Atualize versão no `Cargo.toml`
2. Atualize `CHANGELOG.md`
3. Execute todos os testes
4. Execute benchmarks
5. Crie tag de release
6. Build Docker images
7. Publique no crates.io

### 11. Comunidade

#### Comunicação
- **GitHub Issues**: Bugs e feature requests
- **GitHub Discussions**: Perguntas gerais
- **Discord**: Chat em tempo real
- **Email**: Contato direto

#### Code of Conduct
- Seja respeitoso e inclusivo
- Foque no código, não na pessoa
- Aceite feedback construtivo
- Ajude outros contribuidores

### 12. Recursos Úteis

#### Documentação
- [Rust Book](https://doc.rust-lang.org/book/)
- [Tokio Tutorial](https://tokio.rs/tokio/tutorial)
- [Performance Book](https://nnethercote.github.io/perf-book/)

#### Ferramentas
- [rustfmt](https://github.com/rust-lang/rustfmt)
- [clippy](https://github.com/rust-lang/rust-clippy)
- [cargo-audit](https://github.com/RustSec/rustsec/tree/main/cargo-audit)

## 🙏 Agradecimentos

Obrigado por contribuir para o CrabCache! Sua ajuda torna este projeto melhor para toda a comunidade.

---

Para dúvidas sobre contribuições, abra uma issue ou entre em contato conosco.