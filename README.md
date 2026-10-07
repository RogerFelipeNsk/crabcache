<p align="center"><img src="assets/logo.png" alt="CrabCache" width="200"></p>

# CrabCache

Servidor de cache em memória escrito em Rust, **compatível com o protocolo do Redis** (RESP2).
Funciona com `redis-cli`, `redis-benchmark`, `memtier_benchmark` e as bibliotecas cliente do Redis,
sem adaptação.

> **Status:** v0.2.0, reescrita completa. Só strings, sem persistência. Não use em produção ainda.
> O código anterior (v0.1.x) está na tag `legacy-v1`; veja a errata no [CHANGELOG](CHANGELOG.md).

## Destaques

Medido contra o Redis 8.10 na mesma máquina, com `redis-benchmark` e `memtier_benchmark`
(metodologia e números completos em [docs/BENCHMARKS.md](docs/BENCHMARKS.md)):

* **Mais eficiente por núcleo:** limitado a 1 thread, como o Redis, faz **+23%** de operações sem
  pipeline e **+63% a +84%** com pipeline 16, usando a mesma CPU.
* **Menos memória:** **27% menos** por chave com valores de 10 B, **14%** com 100 B e **4%** com 1 KB.
* **Escala com pipelining:** 4.7M ops/s com pipeline 64, contra ~1.9M do Redis.
* **Baixa latência:** p50 de 23 µs contra 71 µs com um cliente; 167 µs contra 271 µs com 50.
* **Compatível de verdade:** um teste diferencial compara as respostas com um Redis real **byte a
  byte**, incluindo mensagens de erro e casos-limite. 750 mil comandos aleatórios bateram.
* **Seguro por padrão:** escuta só em `127.0.0.1`, AUTH com comparação em tempo constante, limites de
  protocolo iguais aos do Redis e imagem Docker distroless rodando como não-root.

## Início rápido

```bash
cargo build --release
target/release/crabcache            # escuta em 127.0.0.1:6379
redis-cli set saudacao "olá mundo"
redis-cli get saudacao
```

Com Docker:

```bash
docker build -t crabcache .
docker run -p 6379:6379 -e CRABCACHE_REQUIREPASS=troque-isto crabcache
redis-cli -a troque-isto ping
```

## Configuração

Toda opção pode vir da linha de comando ou de variável de ambiente (`crabcache --help`).

| Flag | Variável | Padrão | |
|---|---|---|---|
| `--bind` | `CRABCACHE_BIND` | `127.0.0.1` | Use `0.0.0.0` só com `--requirepass` ou firewall |
| `--port` | `CRABCACHE_PORT` | `6379` | |
| `--threads` | `CRABCACHE_THREADS` | nº de CPUs | |
| `--shards` | `CRABCACHE_SHARDS` | 64 × threads | |
| `--maxmemory` | `CRABCACHE_MAXMEMORY` | `0` (sem limite) | ex.: `512mb`, `4gb` |
| `--maxmemory-policy` | `CRABCACHE_MAXMEMORY_POLICY` | `noeviction` | `allkeys-lru`, `allkeys-lfu`, `allkeys-random` |
| `--maxmemory-samples` | `CRABCACHE_MAXMEMORY_SAMPLES` | `5` | |
| `--requirepass` | `CRABCACHE_REQUIREPASS` | — | |
| `--maxclients` | `CRABCACHE_MAXCLIENTS` | `10000` | |
| `--io-conns-per-thread` | `CRABCACHE_IO_CONNS_PER_THREAD` | `32` | Conexões por thread de I/O antes de ativar outra; `0` = usar todas sempre |
| `--proto-max-bulk-len` | `CRABCACHE_PROTO_MAX_BULK_LEN` | `512mb` | |
| `--client-query-buffer-limit` | `CRABCACHE_CLIENT_QUERY_BUFFER_LIMIT` | `1gb` | |

`maxmemory`, `maxmemory-policy` e `maxmemory-samples` também mudam em tempo de execução com
`CONFIG SET`.

## Comandos suportados

* **Strings:** `GET`, `SET` (`NX`/`XX`/`GET`/`EX`/`PX`/`EXAT`/`PXAT`/`KEEPTTL`), `SETNX`, `SETEX`,
  `PSETEX`, `GETSET`, `GETDEL`, `GETEX`, `MGET`, `MSET`, `MSETNX`, `INCR`, `DECR`, `INCRBY`, `DECRBY`,
  `APPEND`, `STRLEN`, `GETRANGE`/`SUBSTR`
* **Chaves:** `DEL`, `UNLINK`, `EXISTS`, `TOUCH`, `EXPIRE`/`PEXPIRE`/`EXPIREAT`/`PEXPIREAT`
  (`NX`/`XX`/`GT`/`LT`), `TTL`, `PTTL`, `EXPIRETIME`, `PEXPIRETIME`, `PERSIST`, `TYPE`, `KEYS`, `SCAN`,
  `RANDOMKEY`, `RENAME`, `RENAMENX`, `DBSIZE`, `FLUSHDB`, `FLUSHALL`
* **Conexão e servidor:** `PING`, `ECHO`, `SELECT 0`, `QUIT`, `RESET`, `AUTH`, `HELLO 2`, `CLIENT`,
  `COMMAND`, `CONFIG GET/SET/RESETSTAT`, `INFO`, `TIME`, `MEMORY USAGE`

Ainda não há listas, hashes, sets, sorted sets, transações (`MULTI`), pub/sub, Lua, RESP3,
persistência nem replicação. Detalhes em [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

## Testes

```bash
cargo nextest run      # ou: cargo test
```

* Testes unitários do parser, do store, da expiração e da eviction.
* `tests/integration.rs`: servidor real sobre TCP com o cliente oficial `redis` do Rust.
* `tests/differential.rs`: comandos aleatórios contra um Redis real, com respostas comparadas byte a
  byte. Usa o `redis-server` do PATH, ou `CRABCACHE_DIFF_REDIS=host:porta` (que recebe `FLUSHALL`).
  Sem Redis disponível o teste avisa e pula; com `CRABCACHE_REQUIRE_REDIS=1` (como no CI) ele falha.

O CI roda fmt, clippy com `-D warnings`, todos os testes em Linux e macOS, `cargo audit` e um smoke test
da imagem Docker. Nenhum passo usa `continue-on-error`.

## Roadmap

1. Tipos de dados: hashes, listas, sets, sorted sets.
2. Armazenamento ainda mais compacto (alocação em slabs por shard, valores pequenos inline).
3. Persistência (snapshot + log) e `MULTI`/`EXEC`.
4. Formatos de valor compactos (ex.: TOON, compressão) como diferencial.
5. Benchmarks em Linux com cliente e servidor em máquinas separadas, e migração de conexões entre
   threads.

## Licença

MIT. Veja [LICENSE](LICENSE).
