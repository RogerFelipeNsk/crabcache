<p align="center"><img src="assets/logo.png" alt="CrabCache" width="200"></p>

<h1 align="center">CrabCache</h1>

<p align="center">
  Servidor de cache em memória escrito em Rust, compatível com o protocolo do Redis.<br>
  CrabPack: compressão opcional de valores ociosos com dicionários por prefixo de chave.
</p>

<p align="center">
  <a href="https://github.com/RogerFelipeNsk/crabcache/actions/workflows/ci.yml"><img src="https://github.com/RogerFelipeNsk/crabcache/actions/workflows/ci.yml/badge.svg?branch=main" alt="CI"></a>
  <a href="https://github.com/RogerFelipeNsk/crabcache/releases"><img src="https://img.shields.io/github/v/release/RogerFelipeNsk/crabcache" alt="Release"></a>
  <a href="https://hub.docker.com/r/rogerfelipensk/crabcache"><img src="https://img.shields.io/docker/v/rogerfelipensk/crabcache?sort=semver&label=docker" alt="Docker"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue" alt="Licença MIT"></a>
</p>

O CrabCache fala RESP2 e RESP3 e pode ser usado com `redis-cli`, `redis-benchmark`,
`memtier_benchmark` e bibliotecas cliente do Redis, usando os [comandos suportados](#comandos-suportados).

> **Status:** v0.3. Projeto educacional. Só strings e sem persistência; ainda não é para produção.
> A versão 0.1 está na tag [`legacy-v1`](https://github.com/RogerFelipeNsk/crabcache/tree/legacy-v1);
> veja a errata no [CHANGELOG](CHANGELOG.md) e o guia de [migração](#migrando-da-01).

<!-- crabpack-memory-summary:start -->
**Novidade na v0.3.0 — CrabPack:** compressão opcional de valores semelhantes, como sessões e
respostas de API. Na auditoria de 09/10/2026, com **300 mil chaves por conjunto**, a economia mediana
de RAM incremental foi de **55–63% em relação ao Redis**, nos JSONs sintéticos medidos.
Esse ganho depende do volume e dos dados: bases pequenas podem consumir mais RAM após o treino,
e descomprimir custa CPU. Veja [CPU, latência e memória](#cpu-latência-e-memória-do-crabpack).
A opção vem desligada; ative por flag, variável de ambiente ou `CONFIG SET`; veja [como usar](#ativando-o-crabpack).
<!-- crabpack-memory-summary:end -->

**Tutorial completo:** siga o [guia manual de CrabCache e CrabPack](docs/TUTORIAL.md),
do primeiro `PING` à compressão de 5.000 JSONs, com validação dos valores, TTL e memória.

## Desempenho

Auditoria de 09/10/2026 com CrabCache 0.3.0 e Redis 8.10.2, no Apple M1 Pro/macOS.
Os gráficos mostram a **mediana de três rodadas**, com instâncias novas e o mesmo conjunto de chaves
preenchido antes da medição. Todas as leituras medidas tiveram resultado (zero misses).

<p align="center"><img src="docs/img/bench-per-core.svg" alt="Throughput de Redis e CrabCache com uma thread de I/O, mediana de três rodadas auditadas." width="720"></p>

O CrabCache usa `--threads 1` nessa comparação. Isso configura uma thread de I/O; não fixa o processo
a um núcleo. O consumo de CPU de todo o processo também é registrado.

<p align="center"><img src="docs/img/bench-memory.svg" alt="Memória física incremental por chave, descontado o baseline, em três rodadas auditadas." width="720"></p>

Resultados por rodada, faixas de variação, configuração padrão, latências e limitações estão em
[docs/BENCHMARKS.md](docs/BENCHMARKS.md). As [saídas brutas](docs/benchmark-results/2026-10-09)
acompanham os resumos; os gráficos são gerados diretamente desses resumos. Cliente e servidor
compartilham esta máquina, então os resultados não demonstram escala em máquinas separadas.

## CrabPack: compressão que aprende com os seus dados

Valores de cache costumam ser pequenos (centenas de bytes) e parecidos entre si: mesmos campos, mesmos
formatos, mesmos enums. O CrabPack usa essa estrutura **em comum** para comprimir valores
individualmente com um dicionário compartilhado:

1. Agrupa as chaves pelo prefixo (`user:`, `session:`, `product:`…) e coleta amostras em background.
2. Treina um dicionário zstd por prefixo e só o mantém se ele comprimir amostras separadas em pelo
   menos 1,25× (redução de 20% no tamanho) e estimar economia incluindo as estruturas
   preparadas do dicionário. A seleção adaptativa compara tamanhos e níveis de compressão.
3. Comprime os valores que ficaram ociosos (`--compression-min-idle`, 60 s por padrão). Chaves acessadas
   com frequência continuam sem compressão, e qualquer escrita grava o valor sem compressão de novo.
4. Descomprime de forma transparente: o `GET` devolve exatamente os bytes gravados, e nenhum cliente
   muda nada.

<p align="center"><img src="docs/img/bench-crabpack.svg" alt="Memória incremental por chave com JSON sintético de sessões, produtos e API; mediana de três rodadas, após comprimir 100% das chaves." width="720"></p>

O `GET` descomprime para responder e mantém o valor comprimido no armazenamento. Alterar só o TTL
também preserva a representação; o comportamento das escritas acima se refere ao conteúdo do valor.

<!-- crabpack-read-summary:start -->
### CPU, latência e memória do CrabPack

Ensaio complementar de **11/10/2026**: **10.000 chaves por conjunto**, três rodadas
de 4 s, CrabCache 0.3.0 nativo no Apple M1 Pro/macOS, uma thread de I/O e loopback.
Zero misses; os valores foram conferidos byte a byte. Estes dados não são a mesma carga da
auditoria de 300 mil chaves acima. Médias ponderadas pelo número de GETs; **1.000 µs = 1 ms**.

| Objeto JSON | Valor médio | GET individual sem / com CrabPack (µs) | CPU sob carga sem / com CrabPack (µs CPU/GET) |
|---|---|---|---|
| Sessão JSON | 298 B | 24,87 / 25,45 | 0,674 / 1,033 |
| Produto JSON | 281 B | 24,95 / 26,44 | 0,647 / 1,024 |
| API JSON | 314 B | 22,07 / 25,00 | 0,669 / 1,027 |
| API JSON · 40 itens | 3913 B | 24,86 / 27,93 | 2,050 / 3,702 |

Com **48 conexões e pipeline 16**, a diferença de custo de CPU por GET foi de **+53% a +81%** com CrabPack.
GET inclui comunicação local e resposta; CPU/GET mede o processo servidor, incluindo threads auxiliares.
O cliente recebe o valor original: a compressão do armazenamento não reduz o tráfego do GET.

<p align="center"><img src="docs/analises/leituras-crabpack-2026-10-11/resumo-readme.svg" alt="Médias de latência individual e CPU por GET, CrabPack desligado e ligado, para quatro conjuntos JSON." width="960"></p>

**Base de 10.000 chaves:** a RAM observada após o treino ficou maior com CrabPack em Sessão JSON, Produto JSON, API JSON.
A amostra pós-treino ficou menor com CrabPack em API JSON · 40 itens.
Uma verificação adicional de sessões acompanhou o efeito por 20 s, e a RAM continuou maior nesse intervalo.
Comprimir os dados contabilizados não garante diminuir a RAM física do processo.
Essas amostras não determinam o pico nem o consumo em prazos maiores; os custos não foram isolados.

O [relatório completo](docs/analises/leituras-crabpack-2026-10-11/RELATORIO.md) mostra p99, faixas de variação,
RAM, exemplos de objetos e tempo de converter JSON no cliente. As [medições e saídas brutas](docs/benchmark-results/2026-10-11-read-analysis)
acompanham a documentação. O resumo e este gráfico são gerados desses dados, e o CI rejeita divergências.
<!-- crabpack-read-summary:end -->

### Ativando o CrabPack

No servidor local, ative a compressão mantendo o padrão de 60 s sem acesso:

```bash
target/release/crabcache --compression
```

Na imagem da v0.5.0, use a variável de ambiente:

```bash
docker run -d --name crabcache-pack -p 127.0.0.1:6379:6379 \
  -e CRABCACHE_REQUIREPASS=troque-esta-senha \
  -e CRABCACHE_COMPRESSION=true \
  rogerfelipensk/crabcache:v0.5.0
```

Em um servidor local já em execução, configure e acompanhe pelo protocolo Redis
(se houver senha, use `REDISCLI_AUTH`):

```bash
redis-cli CONFIG SET compression yes
redis-cli CONFIG SET compression-algorithm auto
redis-cli CONFIG SET compression-min-idle 60
redis-cli CONFIG SET compression-min-size 64
redis-cli CONFIG GET 'compression*'
redis-cli info compression
# Exemplo de saída; os valores dependem do conjunto de dados.
# compression_dicts:1
# compressed_keys:300000
# compression_ratio:3.61
# dict0:prefix=session:,trained_ratio=3.61
```

A compressão é automática e gradual. O treino precisa acumular amostras de valores do mesmo
prefixo (até 1.000 amostras ou 256 KiB), e só é aceito se atingir o ganho mínimo. Valores menores
que 64 B não são candidatos por padrão. Uma ou duas chaves pequenas não demonstram o benefício;
`compressed_keys:0` logo após ativar não indica, por si só, uma falha.

Para desligar a compactação de novos valores, execute `redis-cli CONFIG SET compression no`.
Os valores já comprimidos continuam legíveis. `CONFIG SET` vale para o processo atual; para manter
a opção ao reiniciar, configure a flag ou as variáveis de ambiente no serviço/container.

A v0.5.0 adiciona seleção adaptativa entre Zstd, LZ4 e Plain (`compression-algorithm`),
dicionários dinâmicos com retreinamento automático em mudanças de estrutura e controle por
custo-benefício líquido e latência P95/P99 (`compression-latency-target-us`). Controles, medições
locais e limitações estão em [docs/CRABPACK_V2.md](docs/CRABPACK_V2.md) e
[docs/CRABPACK_ADAPTIVE.md](docs/CRABPACK_ADAPTIVE.md); os gráficos acima preservam a campanha
histórica da v0.3.0.

### Demonstração local

Em uma instância local descartável, use o conjunto sintético incluído no projeto. Primeiro inicie
o servidor em outro terminal:

```bash
cargo build --release --bin crabcache --example dataset
target/release/crabcache --port 7379 --compression --compression-min-idle 0
```

Então carregue as sessões e acompanhe a compactação durante alguns segundos:

```bash
target/release/examples/dataset session 3000 | redis-cli -p 7379 --pipe
redis-cli -p 7379 INFO compression
redis-cli -p 7379 GET session:42
```

O GET retorna o JSON original. `--compression-min-idle 0` serve para esta demonstração imediata;
o padrão de 60 s concentra a compactação em valores frios. O comando GET de clientes existentes
continua igual, sem habilitar um protocolo especial.

**Custos e limites:**

* Descomprimir custa CPU. A tabela de GET em [BENCHMARKS.md](docs/BENCHMARKS.md) compara leituras
  com e sem compressão, conferindo também todos os valores retornados fora do intervalo medido.
* A compactação inicial usa memória temporária. As amostras depois da carga não capturam
  necessariamente o pico; não devem ser usadas como limite máximo de memória.
* Os conjuntos de JSON são **sintéticos e determinísticos**, não dados coletados de produção.
* Dados que não atingem o ganho mínimo podem ter o dicionário rejeitado; valores pequenos ou que
  não economizam espaço são mantidos sem compressão.

Por isso a compressão ainda é opcional nesta versão. Detalhes em [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md#compressão-crabpack)
e números completos em [docs/BENCHMARKS.md](docs/BENCHMARKS.md#crabpack-json-sintético).

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

**A partir do código** (Rust 1.99+):

```bash
cargo build --release
target/release/crabcache              # escuta em 127.0.0.1:6379
redis-cli set saudacao "olá mundo"
redis-cli get saudacao
```

## Usando com seu cliente Redis

Use os clientes Redis com os comandos suportados nesta versão. Os exemplos abaixo foram testados
contra o CrabCache 0.2.1; o [tutorial da 0.3.0](docs/TUTORIAL.md#10-usar-em-uma-aplicação-nodejs-opcional)
também valida o Node.js com `redis` 4.7.1. Confira as estruturas e comandos usados pelo seu framework.

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
| `--maxmemory` | `CRABCACHE_MAXMEMORY` | `0` (sem limite) | Limite dos dados contabilizados; não da RAM total. Ex.: `512mb`, `4gb` |
| `--maxmemory-policy` | `CRABCACHE_MAXMEMORY_POLICY` | `noeviction` | `allkeys-lru`, `allkeys-lfu`, `allkeys-random` |
| `--maxmemory-samples` | `CRABCACHE_MAXMEMORY_SAMPLES` | `5` | |
| `--requirepass` | `CRABCACHE_REQUIREPASS` | — | |
| `--maxclients` | `CRABCACHE_MAXCLIENTS` | `10000` | |
| `--proto-max-bulk-len` | `CRABCACHE_PROTO_MAX_BULK_LEN` | `512mb` | |
| `--client-query-buffer-limit` | `CRABCACHE_CLIENT_QUERY_BUFFER_LIMIT` | `1gb` | |
| `--compression` | `CRABCACHE_COMPRESSION` | desligado | Ativa o CrabPack |
| `--compression-min-idle` | `CRABCACHE_COMPRESSION_MIN_IDLE` | `60` | Segundos sem acesso antes de comprimir |
| `--compression-min-size` | `CRABCACHE_COMPRESSION_MIN_SIZE` | `64` | Valores menores nunca são comprimidos |

`maxmemory`, `maxmemory-policy`, `maxmemory-samples`, `compression`, `compression-min-idle` e
`compression-min-size` também podem mudar em tempo de execução com `CONFIG SET`. Para usar como cache com limite de memória:

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
* **CrabPack:** dicionários zstd treinados por prefixo de chave comprimem valores ociosos; a entrada
  marca o valor como comprimido e guarda o dicionário e o tamanho original.
* **Parser incremental** com os mesmos limites de protocolo do Redis, para que um cliente lento ou
  malicioso não consuma CPU nem memória sem limite.

Detalhes em [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

## Testes

```bash
cargo nextest run      # ou: cargo test
```

* **Testes diferenciais** (`tests/differential.rs`): sequências aleatórias de comandos são enviadas a
  um Redis real e ao CrabCache, em RESP2 e em RESP3, e as respostas precisam ser idênticas **byte a
  byte**, inclusive as mensagens de erro, com tolerância numérica para TTL e ordenação de `KEYS`.
  Na auditoria local, 1,53 milhão de comandos gerados passaram contra Redis 8.10.2 no macOS.
  Valores e TTLs finais também são conferidos. Sem Redis disponível, o teste avisa e pula; com `CRABCACHE_REQUIRE_REDIS=1` (como no CI), ele
  falha.
* **Testes de integração** (`tests/integration.rs`): servidor real sobre TCP com o cliente
  `redis` do Rust, cobrindo valores binários, pipelining, concorrência, expiração, eviction e limites
  de protocolo.
* Testes unitários do parser, do armazenamento, da expiração e da eviction.

O CI roda fmt, clippy com `-D warnings`, todos os testes em Linux e macOS, `cargo audit` e um teste da
imagem Docker. Nenhum passo tolera falha.
O smoke test da imagem também comprime 2.000 valores binários, confere todos os bytes e valida TTL,
leitura com compressão desligada e uma escrita sobre valor previamente comprimido.

Para reproduzir os benchmarks: `scripts/bench.sh`, `scripts/bench-1cpu.sh`,
`scripts/bench-memory.sh` e `scripts/bench-compression.sh` (instruções em [docs/BENCHMARKS.md](docs/BENCHMARKS.md)).

### Manter os resultados da documentação consistentes

Os resultados têm data, conjunto, versão e ambiente definidos. O CI valida as medições publicadas
e a geração dos documentos; ele não executa benchmarks de desempenho para inventar números novos.
Depois de registrar uma nova medição auditável, regenere o resumo e confira a documentação:

```bash
python3 scripts/report-read-analysis.py
python3 scripts/report-read-analysis.py --check
```

O comando `--check` não altera arquivos: confere os 96 registros do ensaio complementar, as cargas,
os hashes dos dados, as fórmulas de CPU/latência, o CSV/JSON e o resumo/gráfico do README.
O [guia das medições publicadas](docs/benchmark-results/2026-10-11-read-analysis/README.md)
explica a atualização dos gráficos completos e como registrar rodadas futuras.

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
3. CrabPack fase 2: retreino de dicionários quando os dados mudam, descompressão mais rápida e um
   formato estruturado para JSON (consultas como `JSON.GET user:1 $.email` sem descomprimir tudo).
4. Armazenamento ainda mais compacto: alocação em slabs por shard e valores pequenos inline.
5. Benchmarks em Linux com cliente e servidor em máquinas separadas, e migração de conexões entre
   threads.

## Licença

MIT. Veja [LICENSE](LICENSE).
