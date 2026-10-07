<p align="center"><img src="assets/logo.png" alt="CrabCache" width="200"></p>

<h1 align="center">CrabCache</h1>

<p align="center">
  Servidor de cache em memória escrito em Rust, compatível com o protocolo do Redis.<br>
  Mais eficiente por núcleo e mais econômico em memória que o Redis 8.10, medido na mesma máquina.
</p>

<p align="center">
  <a href="https://github.com/RogerFelipeNsk/crabcache/actions/workflows/ci.yml"><img src="https://github.com/RogerFelipeNsk/crabcache/actions/workflows/ci.yml/badge.svg?branch=main" alt="CI"></a>
  <a href="https://github.com/RogerFelipeNsk/crabcache/releases"><img src="https://img.shields.io/github/v/release/RogerFelipeNsk/crabcache" alt="Release"></a>
  <a href="https://hub.docker.com/r/rogerfelipensk/crabcache"><img src="https://img.shields.io/docker/v/rogerfelipensk/crabcache?sort=semver&label=docker" alt="Docker"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue" alt="Licença MIT"></a>
</p>

O CrabCache fala RESP2 e RESP3, então `redis-cli`, `redis-benchmark`, `memtier_benchmark` e as
bibliotecas cliente do Redis funcionam sem nenhuma adaptação.

> **Status:** v0.2, reescrita completa. Só strings e sem persistência; ainda não é para produção.
> A versão 0.1 está na tag [`legacy-v1`](https://github.com/RogerFelipeNsk/crabcache/tree/legacy-v1);
> veja a errata no [CHANGELOG](CHANGELOG.md) e o guia de [migração](#migrando-da-01).

## Desempenho

Medições com ferramentas oficiais do Redis, mesmos parâmetros, mesma máquina (Apple M1 Pro). A
metodologia completa e os números brutos estão em [docs/BENCHMARKS.md](docs/BENCHMARKS.md).

<p align="center"><img src="docs/img/bench-per-core.svg" alt="Throughput com o mesmo núcleo: sem pipeline, Redis 112k ops/s e CrabCache 136k ops/s (+21%); com pipeline de 16 comandos, Redis 924k ops/s e CrabCache 1,71M ops/s (+85%)." width="720"></p>

O Redis executa comandos numa única thread. Para a comparação ser justa, o CrabCache roda com
`--threads 1`, e a CPU de cada servidor foi medida durante o teste: os dois usaram ~1 núcleo.

<p align="center"><img src="docs/img/bench-memory.svg" alt="Memória por chave: valores de 10 B, Redis 86 B e CrabCache 63 B (−27%); valores de 100 B, Redis 184 B e CrabCache 159 B (−14%); valores de 1 KB, Redis 1110 B e CrabCache 1066 B (−4%)." width="720"></p>

Com a configuração padrão, usando o `redis-benchmark` (valores de 100 B):

| Cenário | Redis 8.10 | CrabCache 0.2 |
|---|---|---|
| 1 conexão | 13.3k req/s · p50 71 µs | **43.4k req/s · p50 23 µs** |
| 50 conexões | 147k req/s · p50 271 µs | **150k req/s · p50 167 µs** |
| 50 conexões, pipeline 16 | 1.01M / 1.25M (SET/GET) | **1.72M / 1.60M** |
| 50 conexões, pipeline 64 | 1.47M / 1.87M | **4.73M / 4.77M** |

Com 50 conexões sem pipeline, o próprio `redis-benchmark` (single-thread) limita os dois em ~150k;
a diferença aparece na latência.

## Início rápido

**Docker** (imagem multi-arquitetura para amd64 e arm64):

```bash
docker run -d --name crabcache -p 6379:6379 \
  -e CRABCACHE_REQUIREPASS=troque-esta-senha \
  rogerfelipensk/crabcache:latest

redis-cli -a troque-esta-senha ping
```

A imagem é distroless (sem shell), roda como usuário não-root e tem ~36 MB. Dentro do container o
servidor escuta em `0.0.0.0`, por isso defina sempre `CRABCACHE_REQUIREPASS` se a porta puder ser
acessada de fora.

**A partir do código** (Rust 1.85+):

```bash
cargo build --release
target/release/crabcache              # escuta em 127.0.0.1:6379
redis-cli set saudacao "olá mundo"
redis-cli get saudacao
```

## Usando com seu cliente Redis

Qualquer cliente Redis funciona; os abaixo foram testados contra o CrabCache 0.2.1.

**Node.js** (`redis` 4.x):

```js
import { createClient } from "redis";

const client = await createClient({ url: "redis://localhost:6379" }).connect();
await client.set("sessao:42", "dados", { EX: 3600 });
console.log(await client.get("sessao:42"));
```

**Python** (`redis` 8.x, que negocia RESP3 automaticamente):

```python
import redis

r = redis.Redis(host="localhost", port=6379, decode_responses=True)
r.set("sessao:42", "dados", ex=3600)
print(r.get("sessao:42"))
```

**Rust** (`redis` 1.x, usado nos testes de integração):

```rust
let mut con = redis::Client::open("redis://127.0.0.1:6379/")?.get_connection()?;
redis::cmd("SET").arg("sessao:42").arg("dados").arg("EX").arg(3600).query::<()>(&mut con)?;
let v: String = redis::cmd("GET").arg("sessao:42").query(&mut con)?;
```

## Configuração

Toda opção pode vir da linha de comando ou de variável de ambiente (`crabcache --help`).

| Flag | Variável | Padrão | |
|---|---|---|---|
| `--bind` | `CRABCACHE_BIND` | `127.0.0.1` | Use `0.0.0.0` só com `--requirepass` ou firewall |
| `--port` | `CRABCACHE_PORT` | `6379` | |
| `--threads` | `CRABCACHE_THREADS` | nº de CPUs | Threads de I/O |
| `--io-conns-per-thread` | `CRABCACHE_IO_CONNS_PER_THREAD` | `32` | Conexões por thread antes de ativar outra; `0` = usar todas sempre |
| `--shards` | `CRABCACHE_SHARDS` | 64 × threads | |
| `--maxmemory` | `CRABCACHE_MAXMEMORY` | `0` (sem limite) | ex.: `512mb`, `4gb` |
| `--maxmemory-policy` | `CRABCACHE_MAXMEMORY_POLICY` | `noeviction` | `allkeys-lru`, `allkeys-lfu`, `allkeys-random` |
| `--maxmemory-samples` | `CRABCACHE_MAXMEMORY_SAMPLES` | `5` | |
| `--requirepass` | `CRABCACHE_REQUIREPASS` | — | |
| `--maxclients` | `CRABCACHE_MAXCLIENTS` | `10000` | |
| `--proto-max-bulk-len` | `CRABCACHE_PROTO_MAX_BULK_LEN` | `512mb` | |
| `--client-query-buffer-limit` | `CRABCACHE_CLIENT_QUERY_BUFFER_LIMIT` | `1gb` | |

`maxmemory`, `maxmemory-policy` e `maxmemory-samples` também podem mudar em tempo de execução com
`CONFIG SET`. Para usar como cache com limite de memória:

```bash
crabcache --maxmemory 2gb --maxmemory-policy allkeys-lfu
```

## Comandos suportados

* **Strings:** `GET`, `SET` (`NX`/`XX`/`GET`/`EX`/`PX`/`EXAT`/`PXAT`/`KEEPTTL`), `SETNX`, `SETEX`,
  `PSETEX`, `GETSET`, `GETDEL`, `GETEX`, `MGET`, `MSET`, `MSETNX`, `INCR`, `DECR`, `INCRBY`, `DECRBY`,
  `APPEND`, `STRLEN`, `GETRANGE`/`SUBSTR`
* **Chaves:** `DEL`, `UNLINK`, `EXISTS`, `TOUCH`, `EXPIRE`/`PEXPIRE`/`EXPIREAT`/`PEXPIREAT`
  (`NX`/`XX`/`GT`/`LT`), `TTL`, `PTTL`, `EXPIRETIME`, `PEXPIRETIME`, `PERSIST`, `TYPE`, `KEYS`, `SCAN`,
  `RANDOMKEY`, `RENAME`, `RENAMENX`, `DBSIZE`, `FLUSHDB`, `FLUSHALL`
* **Conexão e servidor:** `PING`, `ECHO`, `SELECT 0`, `QUIT`, `RESET`, `AUTH`, `HELLO 2|3`, `CLIENT`,
  `COMMAND`, `CONFIG GET/SET/RESETSTAT`, `INFO`, `TIME`, `MEMORY USAGE`

Ainda não há listas, hashes, sets, sorted sets, transações (`MULTI`), pub/sub, Lua, persistência nem
replicação.

## Como funciona

* **Thread-per-core:** cada thread de I/O tem seu próprio runtime e seu próprio `epoll`/`kqueue`. Uma
  conexão é lida, executada e respondida na mesma thread, e as respostas de um lote de comandos saem
  numa única escrita.
* **Distribuição adaptativa:** threads são ativadas conforme o número de conexões cresce, porque
  espalhar carga leve por muitas threads custa mais em despertares do que ganha em paralelismo.
* **Armazenamento compacto:** cada chave ocupa uma entrada de 16 bytes, mais uma única alocação com
  chave, valor e TTL (que só existe se a chave expirar). As entradas ficam em blocos fixos, sem a folga
  e as cópias de um vetor que cresce.
* **Parser incremental** com os mesmos limites de protocolo do Redis, para que um cliente lento ou
  malicioso não consuma CPU nem memória sem limite.

Detalhes em [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

## Testes

```bash
cargo nextest run      # ou: cargo test
```

* **Testes diferenciais** (`tests/differential.rs`): sequências aleatórias de comandos são enviadas a
  um Redis real e ao CrabCache, em RESP2 e em RESP3, e as respostas precisam ser idênticas **byte a
  byte**, inclusive as mensagens de erro. 750 mil comandos bateram com o Redis 8.10 no Linux e no
  macOS. Sem Redis disponível, o teste avisa e pula; com `CRABCACHE_REQUIRE_REDIS=1` (como no CI), ele
  falha.
* **Testes de integração** (`tests/integration.rs`): servidor real sobre TCP com o cliente oficial
  `redis` do Rust, cobrindo valores binários, pipelining, concorrência, expiração, eviction e limites
  de protocolo.
* Testes unitários do parser, do armazenamento, da expiração e da eviction.

O CI roda fmt, clippy com `-D warnings`, todos os testes em Linux e macOS, `cargo audit` e um teste da
imagem Docker. Nenhum passo tolera falha.

Para reproduzir os benchmarks: `scripts/bench.sh`, `scripts/bench-1cpu.sh` e
`scripts/bench-memory.sh` (instruções em [docs/BENCHMARKS.md](docs/BENCHMARKS.md)).

## Migrando da 0.1

A 0.2 é incompatível com a 0.1:

| 0.1 | 0.2 |
|---|---|
| Protocolo de texto próprio (`PUT k v`) e cliente `crabcache-client-js` | Protocolo do Redis: use qualquer cliente Redis (`SET k v`) |
| Porta `8000` | Porta `6379` |
| Métricas HTTP na porta `9090` | `INFO` pelo próprio protocolo |
| `CRABCACHE_BIND_ADDR` | `CRABCACHE_BIND` |
| `CRABCACHE_SERVER_TYPE`, protocolo TOON | Removidos: há um único servidor |
| Escuta em `0.0.0.0` por padrão | Escuta em `127.0.0.1` por padrão |

## Roadmap

1. Tipos de dados: hashes, listas, sets e sorted sets.
2. Persistência (snapshot + log) e `MULTI`/`EXEC`.
3. Armazenamento ainda mais compacto: alocação em slabs por shard e valores pequenos inline.
4. Formatos de valor compactos (como TOON e compressão) como diferencial.
5. Benchmarks em Linux com cliente e servidor em máquinas separadas, e migração de conexões entre
   threads.

## Licença

MIT. Veja [LICENSE](LICENSE).
