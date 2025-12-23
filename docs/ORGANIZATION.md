# CrabCache - Organização do Projeto

## 📁 Estrutura Organizada

O projeto CrabCache foi reorganizado para uma estrutura mais limpa e profissional:

```
crabcache/
├── 📋 ORGANIZATION.md           # Este arquivo - guia de organização
├── 📋 Cargo.toml               # Configuração Rust
├── 📋 Cargo.lock               # Lock de dependências
├── 📋 .gitignore               # Arquivos ignorados pelo Git
│
├── 🐳 docker/                  # TUDO relacionado ao Docker
│   ├── README.md               # Guia completo do Docker
│   ├── Dockerfile              # Container principal
│   ├── Dockerfile.tester       # Container de testes
│   ├── Dockerfile.wrapper      # Container HTTP wrapper
│   ├── requirements-wrapper.txt # Dependências Python
│   └── compose/
│       ├── docker-compose.yml      # Orquestração principal
│       └── docker-compose.redis.yml # Comparação com Redis
│
├── 📚 docs/                    # TODA a documentação
│   ├── API.md                  # Documentação da API
│   ├── api-spec.yaml           # Especificação OpenAPI
│   ├── DOCKER_COMPOSE_README.md # Guia Docker Compose
│   ├── HTTP_WRAPPER_README.md  # Guia HTTP Wrapper
│   ├── INSOMNIA_GUIDE.md       # Guia Insomnia
│   ├── PERFORMANCE_ANALYSIS.md # Análise de performance
│   ├── PIPELINING_EXPLAINED.md # Explicação pipelining
│   ├── insomnia-collection.json # Coleção Insomnia
│   ├── test_api.py             # Testes da API
│   └── NEXT_STEPS.md           # Próximos passos
│
├── 🧪 scripts/                 # TODOS os scripts de teste
│   ├── test_observability.py   # Teste de observabilidade
│   ├── simple_redis_comparison.py # Comparação Redis
│   ├── performance_profiler.py # Profiler de performance
│   ├── tcp_load_test.py        # Teste de carga TCP
│   ├── http_wrapper.py         # HTTP wrapper
│   ├── test_docker.py          # Testes Docker
│   ├── run_p99_tests.sh        # Testes P99
│   ├── benchmark_suite.sh      # Suite de benchmarks
│   └── ... (40+ scripts organizados)
│
├── ⚙️ config/                  # Configurações
│   └── default.toml            # Configuração padrão
│
├── 🏗️ src/                     # Código fonte Rust
│   ├── main.rs                 # Entry point
│   ├── lib.rs                  # Library exports
│   ├── server/                 # Servidor TCP e HTTP
│   ├── protocol/               # Protocolos binário e texto
│   ├── shard/                  # Sistema de sharding
│   ├── store/                  # Armazenamento e zero-copy
│   ├── client/                 # Cliente nativo
│   ├── metrics/                # Sistema de observabilidade
│   ├── utils/                  # Utilitários (SIMD, etc.)
│   ├── eviction/               # Algoritmos de eviction
│   ├── ttl/                    # Sistema TTL
│   └── wal/                    # Write-Ahead Log
│
├── 🧪 tests/                   # Testes Rust
├── 📊 benchmark_results/       # Resultados de benchmarks
├── 🎯 examples/                # Exemplos de uso
├── ⚡ benches/                 # Benchmarks Criterion
├── 🎯 target/                  # Build artifacts (Rust)
└── 🔧 .github/                 # CI/CD workflows
```

## 🎯 Benefícios da Reorganização

### ✅ Antes (Desorganizado)
```
crabcache/
├── Dockerfile                  # Espalhado na raiz
├── docker-compose.yml         # Espalhado na raiz
├── http_wrapper.py            # Script na raiz
├── test_docker.py             # Teste na raiz
├── DOCKER_README.md           # Doc na raiz
├── run_tests.sh               # Script na raiz
└── ... (arquivos espalhados)
```

### ✅ Depois (Organizado)
```
crabcache/
├── docker/                    # TUDO do Docker junto
├── docs/                      # TODA documentação junta
├── scripts/                   # TODOS os scripts juntos
└── src/                       # Código fonte limpo
```

## 📋 Como Usar Cada Pasta

### 🐳 docker/
**Para containerização e deploy**
```bash
cd docker
docker build -f Dockerfile -t crabcache .
docker-compose -f compose/docker-compose.yml up
```

### 📚 docs/
**Para documentação e guias**
- Leia `API.md` para entender a API
- Use `insomnia-collection.json` para testes
- Consulte `PERFORMANCE_ANALYSIS.md` para métricas

### 🧪 scripts/
**Para testes e benchmarks**
```bash
cd scripts
python3 test_observability.py      # Teste completo
python3 simple_redis_comparison.py # Comparar com Redis
./benchmark_suite.sh               # Suite completa
```

### ⚙️ config/
**Para configuração**
- Edite `default.toml` para configurar o CrabCache

### 🏗️ src/
**Para desenvolvimento**
```bash
cargo build --release    # Build
cargo test               # Testes
cargo bench             # Benchmarks
```

## 🎯 Vantagens da Nova Estrutura

### 1. **Clareza**
- Cada tipo de arquivo tem seu lugar
- Fácil encontrar o que precisa
- Estrutura profissional

### 2. **Manutenibilidade**
- Fácil adicionar novos scripts
- Documentação centralizada
- Docker organizado

### 3. **Colaboração**
- Novos desenvolvedores encontram tudo facilmente
- Estrutura padrão da indústria
- Separação clara de responsabilidades

### 4. **Deploy**
- Docker isolado e completo
- Scripts de teste organizados
- Configuração centralizada

## 🚀 Próximos Passos

### Para Desenvolvedores
1. **Desenvolvimento**: Trabalhe em `src/`
2. **Testes**: Use scripts em `scripts/`
3. **Documentação**: Atualize `docs/`
4. **Deploy**: Use `docker/`

### Para Usuários
1. **Instalação**: Use `docker/compose/docker-compose.yml`
2. **API**: Consulte `docs/API.md`
3. **Testes**: Execute `scripts/test_observability.py`
4. **Monitoramento**: Acesse http://localhost:9090/dashboard

## 📊 Estatísticas da Reorganização

### Arquivos Movidos
- **Docker**: 6 arquivos → `docker/`
- **Scripts**: 5 arquivos → `scripts/`
- **Documentação**: 3 arquivos → `docs/`
- **Total**: 14 arquivos organizados

### Resultado
- ✅ Raiz limpa (apenas essenciais)
- ✅ Docker centralizado
- ✅ Scripts organizados
- ✅ Documentação junta
- ✅ Estrutura profissional

---

**🏆 CrabCache agora tem uma estrutura profissional e organizada!**

**Tudo no seu lugar, fácil de encontrar e manter.** 📁✨