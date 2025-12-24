# 🤝 Guia de Contribuição - CrabCache v0.0.1

Obrigado pelo interesse em contribuir para o CrabCache! Este é um projeto educacional desenvolvido para aprendizado de Rust e sistemas de cache.

> **⚠️ Aviso Educacional**: Este projeto foi desenvolvido para fins de aprendizado. Contribuições são bem-vindas para melhorar o valor educacional do projeto.

## 🎯 Objetivo do Projeto

O CrabCache é um projeto educacional que demonstra:
- **Programação em Rust**: Conceitos avançados da linguagem
- **Sistemas de Cache**: Implementação de algoritmos modernos
- **Programação Assíncrona**: Uso do Tokio e async/await
- **Estruturas de Dados**: Lock-free e thread-safe
- **Performance**: Otimizações e benchmarking

## 🚀 Como Começar

### Pré-requisitos

```bash
# Rust 1.92+ (recomendado)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Docker (opcional, para testes)
# Instale via: https://docs.docker.com/get-docker/
```

### Setup do Ambiente

```bash
# 1. Clone o repositório
git clone https://github.com/RogerFelipeNsk/crabcache.git
cd crabcache

# 2. Build do projeto
cargo build

# 3. Execute os testes
cargo test

# 4. Execute os benchmarks
cargo bench

# 5. Teste os exemplos
cargo run --example security_example
cargo run --example wal_example
cargo run --example tinylfu_example
```

### Verificação da Instalação

```bash
# Build release
cargo build --release

# Execute o servidor
./target/release/crabcache

# Em outro terminal, teste a conectividade
echo "PING" | nc localhost 8000
# Deve retornar: PONG
```

## 📁 Estrutura do Projeto

```
crabcache/
├── src/                    # 🦀 Código fonte principal
│   ├── client/            # Cliente nativo Rust
│   ├── config/            # Sistema de configuração
│   ├── eviction/          # Algoritmos TinyLFU
│   ├── metrics/           # Métricas Prometheus
│   ├── protocol/          # Protocolos TCP/Pipeline
│   ├── security/          # Auth/Rate Limit/IP Filter
│   ├── server/            # Servidor TCP assíncrono
│   ├── shard/             # Gerenciamento de shards
│   ├── store/             # HashMap thread-safe
│   ├── ttl/               # Sistema de expiração
│   ├── wal/               # Write-Ahead Log
│   └── utils/             # Utilitários compartilhados
├── config/                # ⚙️ Configurações TOML
├── docs/                  # 📚 Documentação completa
├── examples/              # 💡 Exemplos práticos
├── scripts/               # 🧪 Scripts de teste
├── benches/               # 📊 Benchmarks
├── tests/                 # 🔬 Testes de integração
└── docker/                # 🐳 Dockerfiles
```

## 🛠️ Tipos de Contribuição

### 🐛 Reportar Bugs

```markdown
**Descrição do Bug**
Descrição clara do problema encontrado.

**Reprodução**
Passos para reproduzir o comportamento:
1. Execute '...'
2. Conecte com '....'
3. Envie comando '....'
4. Veja o erro

**Comportamento Esperado**
O que deveria acontecer.

**Ambiente**
- OS: [e.g. macOS, Linux, Windows]
- Rust Version: [e.g. 1.92.0]
- CrabCache Version: [e.g. 0.0.1]
```

### 💡 Sugerir Melhorias

```markdown
**Melhoria Proposta**
Descrição clara da melhoria educacional.

**Motivação**
Por que esta melhoria seria valiosa para aprendizado?

**Implementação Sugerida**
Como você implementaria esta melhoria?

**Alternativas Consideradas**
Outras abordagens que você considerou.
```

### 🔧 Contribuir com Código

1. **Fork** o repositório
2. **Crie** uma branch para sua feature (`git checkout -b feature/AmazingFeature`)
3. **Implemente** suas mudanças
4. **Teste** suas mudanças (`cargo test`)
5. **Commit** suas mudanças (`git commit -m 'Add some AmazingFeature'`)
6. **Push** para a branch (`git push origin feature/AmazingFeature`)
7. **Abra** um Pull Request

## 📋 Guidelines de Desenvolvimento

### 🦀 Padrões de Código Rust

```bash
# Formatação automática
cargo fmt

# Linting
cargo clippy -- -D warnings

# Verificação de documentação
cargo doc --no-deps --open
```

### ✅ Padrões de Qualidade

1. **Código Limpo**
   - Use nomes descritivos para variáveis e funções
   - Mantenha funções pequenas e focadas
   - Adicione comentários para lógica complexa

2. **Tratamento de Erros**
   - Use `Result<T, E>` para operações que podem falhar
   - Implemente `Error` trait para erros customizados
   - Evite `unwrap()` em código de produção

3. **Documentação**
   - Documente todas as APIs públicas
   - Use exemplos em doc comments
   - Mantenha README atualizado

4. **Testes**
   - Escreva testes unitários para novas funcionalidades
   - Adicione testes de integração quando apropriado
   - Mantenha cobertura de testes alta

### 🧪 Executando Testes

```bash
# Testes unitários
cargo test

# Testes específicos
cargo test eviction
cargo test wal
cargo test security

# Testes com output detalhado
cargo test -- --nocapture

# Testes de integração
cargo test --test integration_tests

# Benchmarks
cargo bench
```

### 📊 Benchmarks

```bash
# Benchmark completo
cargo bench

# Benchmark específico
cargo bench --bench cache_benchmark

# Comparação com baseline
python3 scripts/benchmark_comparison.py

# Teste de performance
python3 scripts/test_performance.py
```

## 🎓 Áreas de Contribuição Educacional

### 🧠 Algoritmos e Estruturas de Dados
- **TinyLFU**: Melhorias no algoritmo de eviction
- **Count-Min Sketch**: Otimizações de memória
- **Lock-free HashMap**: Implementações thread-safe
- **TTL Wheel**: Algoritmos de expiração eficientes

### ⚡ Performance e Otimização
- **SIMD Operations**: Operações vetorizadas
- **Zero-copy**: Minimizar alocações
- **Pipeline Processing**: Otimização de throughput
- **Memory Management**: Arena allocators

### 🔐 Segurança e Confiabilidade
- **Authentication**: Sistemas de autenticação
- **Rate Limiting**: Algoritmos de controle de taxa
- **Input Validation**: Validação robusta de entrada
- **Error Handling**: Tratamento gracioso de erros

### 📚 Documentação e Exemplos
- **Tutoriais**: Guias passo-a-passo
- **Exemplos**: Casos de uso práticos
- **Benchmarks**: Análises de performance
- **Diagramas**: Visualizações da arquitetura

## 🔍 Processo de Review

### ✅ Checklist do Pull Request

- [ ] **Código compila** sem warnings
- [ ] **Testes passam** (`cargo test`)
- [ ] **Linting limpo** (`cargo clippy`)
- [ ] **Formatação correta** (`cargo fmt`)
- [ ] **Documentação atualizada**
- [ ] **Exemplos funcionais**
- [ ] **Benchmarks executam**
- [ ] **Changelog atualizado** (se necessário)

### 📝 Template do Pull Request

```markdown
## Descrição
Breve descrição das mudanças implementadas.

## Tipo de Mudança
- [ ] Bug fix (mudança que corrige um problema)
- [ ] Nova feature (mudança que adiciona funcionalidade)
- [ ] Breaking change (mudança que quebra compatibilidade)
- [ ] Melhoria de documentação

## Como Testar
Instruções para testar as mudanças:
1. Execute `cargo test`
2. Execute `cargo run --example example_name`
3. Teste com `python3 scripts/test_script.py`

## Checklist
- [ ] Meu código segue os padrões do projeto
- [ ] Realizei self-review do código
- [ ] Comentei código complexo
- [ ] Atualizei documentação relevante
- [ ] Minhas mudanças não geram novos warnings
- [ ] Adicionei testes que provam que minha correção/feature funciona
- [ ] Testes novos e existentes passam localmente
```

## 🌟 Reconhecimento

Contribuidores são reconhecidos de várias formas:

### 📜 Hall of Fame
- **README.md**: Seção de agradecimentos
- **CHANGELOG.md**: Créditos por versão
- **GitHub**: Contributors page

### 🏆 Tipos de Contribuição
- 🐛 **Bug Hunters**: Encontram e reportam bugs
- 💡 **Feature Creators**: Implementam novas funcionalidades
- 📚 **Documentation Heroes**: Melhoram documentação
- 🧪 **Test Masters**: Adicionam testes e benchmarks
- 🎨 **UX Improvers**: Melhoram experiência do usuário

## 📞 Comunicação

### 💬 Canais de Comunicação
- **GitHub Issues**: Para bugs e feature requests
- **GitHub Discussions**: Para perguntas e discussões
- **Pull Requests**: Para contribuições de código
- **Email**: rogerfelipe.nsk@gmail.com (para questões específicas)

### 🤝 Código de Conduta

Este projeto adere aos princípios de:
- **Respeito**: Trate todos com cortesia e profissionalismo
- **Inclusão**: Bem-vindos desenvolvedores de todos os níveis
- **Colaboração**: Trabalhe junto para melhorar o projeto
- **Aprendizado**: Foque no valor educacional das contribuições

## 🎯 Roadmap de Contribuições

### 🚀 Prioridade Alta
- [ ] **Testes de Integração**: Expandir cobertura de testes
- [ ] **Documentação**: Melhorar guias de aprendizado
- [ ] **Exemplos**: Adicionar mais casos de uso
- [ ] **Performance**: Otimizações de throughput

### 📈 Prioridade Média
- [ ] **Client Libraries**: Clientes em outras linguagens
- [ ] **Monitoring**: Dashboards e alertas
- [ ] **Configuration**: Sistema de configuração mais flexível
- [ ] **Logging**: Sistema de logs estruturados

### 🔮 Futuro
- [ ] **Clustering**: Conceitos de distribuição
- [ ] **Replication**: Implementação educacional
- [ ] **TLS/SSL**: Comunicação segura
- [ ] **Lua Scripts**: Sistema de scripting

## 📚 Recursos de Aprendizado

### 🦀 Rust
- [The Rust Book](https://doc.rust-lang.org/book/)
- [Rust by Example](https://doc.rust-lang.org/rust-by-example/)
- [Tokio Tutorial](https://tokio.rs/tokio/tutorial)

### 🗄️ Sistemas de Cache
- [Redis Documentation](https://redis.io/documentation)
- [TinyLFU Paper](https://arxiv.org/abs/1512.00727)
- [Cache Algorithms](https://en.wikipedia.org/wiki/Cache_replacement_policies)

### 🔧 Ferramentas
- [Cargo Book](https://doc.rust-lang.org/cargo/)
- [Clippy Lints](https://rust-lang.github.io/rust-clippy/master/)
- [Rustfmt Configuration](https://rust-lang.github.io/rustfmt/)

---

**Obrigado por contribuir para o CrabCache!** 🦀✨

Sua contribuição ajuda a tornar este projeto um recurso educacional ainda melhor para a comunidade Rust.