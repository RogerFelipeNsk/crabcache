# Benchmarks

Todas as medições usam ferramentas oficiais e independentes (`redis-benchmark` e `memtier_benchmark`),
com os mesmos parâmetros para os dois servidores, na mesma máquina e na mesma sessão. Os scripts estão
em `scripts/` para qualquer um reproduzir.

## Ambiente

* Apple M1 Pro (10 núcleos), 16 GB, macOS, binários nativos arm64
* Redis 8.10.2 (Homebrew), `--save '' --appendonly no`
* CrabCache 0.2.0 em release
* Cliente e servidores na mesma máquina, via loopback
* Valores de 100 bytes e 100.000 chaves aleatórias, salvo indicação

## 1. Mesmo número de núcleos (`scripts/bench-1cpu.sh`)

O Redis executa comandos numa única thread. Para comparar a eficiência por núcleo, o CrabCache roda
com `--threads 1`, e o `memtier_benchmark` (4 threads, 48 conexões, 90% GET) gera a carga, para que o
cliente não seja o gargalo. A CPU consumida por cada servidor foi medida durante a execução.

| | Núcleos usados | Ops/s sem pipeline | Ops/s com pipeline 16 |
|---|---|---|---|
| Redis 8.10 | 0.97–0.99 | 112k | 0.92–1.10M |
| CrabCache `--threads 1` | 0.93–0.98 | **136k (+23%)** | **1.71–1.86M (+63% a +84%)** |

Só escritas (`--ratio=1:0`): Redis 110k / 919k, CrabCache **135k / 1.71M**.

## 2. Configuração padrão (`scripts/bench.sh`, `redis-benchmark`)

| Cenário | Redis 8.10 | CrabCache |
|---|---|---|
| 1 conexão, sem pipeline | 13.3k rps · p50 71 µs | **43.4k rps · p50 23 µs** |
| 50 conexões, sem pipeline | 147k · p50 271 µs | **150k · p50 167 µs** |
| 50 conexões, pipeline 16 | 1.01M (SET) / 1.25M (GET) | **1.72M / 1.60M** |
| 50 conexões, pipeline 64 | 1.47M / 1.87M | **4.73M / 4.77M** |

Com 50 conexões e sem pipeline, o próprio `redis-benchmark` (single-thread) limita o throughput em
~150k nos dois casos; a diferença aparece na latência.

### Quantas threads usar

O CrabCache tem uma thread de I/O por núcleo, mas só ativa
`ceil(conexões / --io-conns-per-thread)` delas (padrão: 32 conexões por thread). Espalhar uma carga leve
por todas as threads faz cada uma acordar a cada requisição. Medido com `redis-benchmark`, 50 clientes,
sem pipeline, todas as conexões distribuídas igualmente:

| Threads em uso | 1 | 2 | 4 | 10 |
|---|---|---|---|---|
| GET rps | 156k | 141k | 100k | 93k |

### Limite desta máquina

Com mais threads, o servidor deixou de ser o gargalo: com o `memtier` em 4 threads, o cliente usou 387%
de CPU, saturado, enquanto o CrabCache usava 2.5 núcleos e esperava requisições. Medir a escala em
vários núcleos exige cliente e servidor em máquinas separadas; esse é o próximo passo.

## 3. Memória (`scripts/bench-memory.sh`)

Mesmas chaves `key:NNNNNNNNNNNN` carregadas via `redis-cli --pipe` em instâncias novas. A memória física
foi medida com `footprint` (inclui páginas comprimidas pelo macOS; o RSS do `ps` não serve aqui).

| Valor | Chaves | Redis 8.10 | CrabCache 0.2.0 | Diferença |
|---|---|---|---|---|
| 10 B | 1M | 86 B/chave | **63 B/chave** | **−27%** |
| 100 B | 1M | 184 B/chave | **159 B/chave** | **−14%** |
| 1000 B | 300k | 1110 B/chave | **1066 B/chave** | **−4%** |

Para comparação, o CrabCache 0.1 (legado) usava ~253 B/chave com valores de 100 B.

O que fez a diferença (ver `docs/ARCHITECTURE.md`):

| Mudança | 10 B | 100 B |
|---|---|---|
| Entrada de 32 B, vetor que dobra | 85 | 168 |
| + mimalloc devolvendo páginas livres na hora | 71 | 165 |
| + entrada de 16 B e entradas em blocos fixos | **63** | **159** |

## Corretude

Throughput só vale com respostas certas. Antes de qualquer medição:

* `tests/differential.rs` compara respostas byte a byte com um Redis real. 750 mil comandos aleatórios
  (500 seeds) bateram com o Redis 8.10.2 no Linux (imagem oficial) e no macOS (Homebrew).
* `tests/integration.rs` valida, com o cliente oficial `redis` do Rust, valores binários, 10.000
  comandos em pipeline, contadores atômicos com 8 clientes concorrentes, expiração ativa, eviction e
  limites de protocolo.

## Como reproduzir

```bash
cargo build --release
redis-server --port 6379 --save '' --appendonly no &

target/release/crabcache --port 7379 &               # configuração padrão
scripts/bench.sh 7379 6379

target/release/crabcache --port 7379 --threads 1 &   # mesmo número de núcleos que o Redis
scripts/bench-1cpu.sh 7379 6379

scripts/bench-memory.sh                              # sobe as próprias instâncias
```
