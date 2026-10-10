# Benchmarks

Auditoria local de **09/10/2026**. Este documento e os gráficos são gerados a partir dos resumos
em [`benchmark-results/2026-10-09`](benchmark-results/2026-10-09). Cada resumo acompanha os comandos,
saídas stdout/stderr e JSON/CSV das ferramentas. Valores das tabelas são **medianas de três rodadas**;
os colchetes mostram mínimo–máximo das rodadas. Essa faixa não é um intervalo de confiança.
Os JSON/CSV numéricos ficam acessíveis diretamente; comandos e logs estão nos `traces.zip` de cada
conjunto. A validação lê esses arquivos sem precisar extrair os ZIPs.

## Ambiente e método

* Apple M1 Pro, 10 CPUs lógicas, 16 GiB, `macOS-26.6.2-arm64-arm-64bit`.
* `Redis server v=8.10.2 sha=00000000:1 malloc=libc bits=64 build=764b3e45065a66fb`; persistência desativada (`--save '' --appendonly no`).
* `crabcache 0.3.0`, build release; hash do binário e commit-base estão nos resumos.
* `memtier_benchmark v=2.5.1 sha=00000000:0 bits=64 libevent=2.1.13-stable openssl=OpenSSL 3.6.3 9 Jun 2026 prometheus=yes`. Cliente e servidor compartilham a máquina, por loopback.
* Instâncias novas por cenário/rodada; ordem dos servidores invertida na segunda rodada.
* Mesmo conjunto determinístico pré-carregado em cada servidor. `redis-cli --pipe` precisa reportar
  zero erros e exatamente N respostas; `DBSIZE` deve ser N. Tamanho médio calculado por bytes RESP,
  incluindo UTF-8 corretamente; o hash SHA-256 identifica cada conjunto.
* `memtier`: 4 threads × 12 conexões, 10 segundos por rodada, sementes distintas por cliente.
  Chaves sorteadas de 1 a N−1; a chave 0 também é carregada e conferida fora da medição.
* Zero misses exigido no cliente e no servidor. Contagem de GET do servidor conferida com o cliente
  nas leituras de JSON, com diferença limitada a conexões × pipeline para respostas em trânsito
  no encerramento. Conferência integral dos valores em memória e JSON ocorre fora do tempo medido.
* Há aquecimento por leituras de uma amostra fixa; não há fase longa de aquecimento nem afinidade de CPU.
  Outros aplicativos do macOS continuam ativos, e a máquina não é um host dedicado de benchmark.

## Uma thread de I/O

`scripts/bench-1cpu.sh`: CrabCache com `--threads 1`, 100.000 chaves de 100 B, 10% SET/90% GET.
O nome do script indica a configuração da thread; não impõe afinidade a um núcleo.
CPU média = tempo de CPU de todo o processo / tempo de parede; eficiência = respostas contabilizadas pelo cliente /
segundos de CPU. `ps time` mede CPU cumulativa, incluindo threads auxiliares; o intervalo inclui
inicialização/finalização do cliente e sua resolução limita a precisão.

| Pipeline | Servidor | ops/s [mín–máx] | CPU média (núcleos) | ops/segundo de CPU |
|---|---|---|---|---|
| 1 | redis | 125.8k [125.1k–126.4k] | 0.98 [0.98–0.98] | 127.6k [127.0k–128.2k] |
| 1 | crabcache | 127.8k [127.1k–128.3k] | 0.97 [0.97–0.98] | 130.1k [129.6k–130.9k] |
| 1 | Diferença de throughput | **+1.6%** | | |
| 16 | redis | 981.6k [686.4k–1.10M] | 0.96 [0.72–0.98] | 1.01M [862.2k–1.12M] |
| 16 | crabcache | 1.38M [1.26M–1.38M] | 0.97 [0.95–0.98] | 1.40M [1.32M–1.41M] |
| 16 | Diferença de throughput | **+40.2%** | | |

## Configuração padrão

`scripts/bench.sh`: `redis-benchmark`, 100.000 chaves de 100 B; 200.000 pedidos por comando com uma
conexão e 2.000.000 com 50 conexões. SET e GET medidos separadamente. CrabCache usa a configuração
padrão de threads. Cada célula mostra throughput [mín–máx] e a mediana de p50/p99 em milissegundos.

| Conexões | Pipeline | Comando | Redis ops/s · p50/p99 ms | CrabCache ops/s · p50/p99 ms |
|---|---|---|---|---|
| 1 | 1 | SET | 38.9k [38.5k–38.9k] · 0.023/0.047 | 41.2k [40.2k–41.3k] · 0.023/0.047 |
| 1 | 1 | GET | 39.4k [39.3k–39.6k] · 0.023/0.047 | 41.2k [41.1k–41.8k] · 0.023/0.047 |
| 50 | 1 | SET | 117.2k [116.9k–124.0k] · 0.271/0.599 | 130.2k [125.1k–132.7k] · 0.183/0.367 |
| 50 | 1 | GET | 125.8k [121.0k–129.0k] · 0.231/0.567 | 130.1k [129.0k–130.3k] · 0.183/0.359 |
| 50 | 16 | SET | 1.07M [1.03M–1.09M] · 0.647/0.991 | 1.68M [1.62M–1.71M] · 0.247/0.455 |
| 50 | 16 | GET | 1.31M [1.26M–1.31M] · 0.511/0.847 | 1.65M [1.61M–1.68M] · 0.239/0.439 |
| 50 | 64 | SET | 1.43M [1.42M–1.43M] · 2.095/2.463 | 4.33M [4.33M–4.38M] · 0.503/0.951 |
| 50 | 64 | GET | 1.85M [1.84M–1.87M] · 1.567/1.951 | 4.29M [4.24M–4.33M] · 0.423/0.655 |

## Memória

`scripts/bench-memory.sh`: chaves `key:NNNNNNNNNNNN`, 1.000.000 de chaves para valores de 10/100 B,
300.000 para valores de 1000 B. Valores fixos preenchidos com `x`; compressão desativada.
Memória incremental por chave = (footprint final − footprint vazio) / N. O baseline contém custos
fixos do processo. Depois da carga, espera de 2 s e mediana de três amostras de footprint; em seguida,
todos os valores são relidos e comparados byte a byte.

| Valor | Chaves | Redis B/chave [mín–máx] | CrabCache B/chave [mín–máx] | Diferença |
|---|---|---|---|---|
| 10 B | 1,000,000 | 84.6 [82.6–85.7] | 62.9 [62.9–62.9] | -25.7% |
| 100 B | 1,000,000 | 183.3 [183.2–184.3] | 159.4 [158.3–159.4] | -13.0% |
| 1000 B | 300,000 | 1110.7 [1107.1–1110.7] | 1066.1 [1066.0–1066.1] | -4.0% |

## CrabPack: JSON sintético

`scripts/bench-compression.sh`: `examples/dataset.rs` gera JSON sintético determinístico de sessões,
produtos e respostas de API. São 300.000 chaves por conjunto em cada instância. CrabPack usa
`--compression --compression-min-idle 0`; **100%** das chaves devem estar comprimidas antes da medição,
ou a rodada falha. Após medir memória, todas as 300.000 chaves são relidas e comparadas ao conjunto
original. A taxa é bytes originais / bytes comprimidos do payload, não a razão do footprint total.

| Conjunto | Média do valor | Redis B/chave [mín–máx] | CrabCache B/chave [mín–máx] | CrabPack B/chave [mín–máx] | Pack vs Redis | Taxa do payload |
|---|---|---|---|---|---|---|
| session | 297.25 B | 415.2 [415.1–418.6] | 391.4 [391.4–391.5] | 160.8 [160.7–160.8] | -61.3% | 3.62× |
| product | 281.04 B | 401.0 [397.7–401.1] | 363.5 [363.5–363.5] | 146.8 [146.7–146.8] | -63.4% | 4.08× |
| api | 315.06 B | 411.7 [411.6–414.7] | 387.9 [387.8–388.0] | 185.3 [185.2–213.2] | -55.0% | 3.59× |

Leituras aleatórias de `session:*`, 100% hits, uma thread de I/O, 48 conexões, 10 s/rodada.
Após cada rodada, todos os valores são conferidos; no CrabPack, a contagem de comprimidos deve
continuar em 300.000. Cada célula mostra throughput [mín–máx] e mediana de p50/p99 em ms.

| Pipeline | Redis ops/s · p50/p99 ms | CrabCache ops/s · p50/p99 ms | CrabPack ops/s · p50/p99 ms |
|---|---|---|---|
| 1 | 125.5k [121.7k–125.7k] · 0.383/0.607 | 128.2k [127.3k–129.0k] · 0.359/0.511 | 116.2k [115.6k–118.7k] · 0.415/0.575 |
| 16 | 1.01M [1.01M–1.03M] · 0.743/1.255 | 1.14M [1.14M–1.15M] · 0.663/1.007 | 725.5k [722.8k–742.0k] · 1.023/1.439 |

## Corretude e auditoria

* Suíte original passou antes das mudanças. A suíte reforçada passou em **release e debug**:
  32 testes unitários, 2 diferenciais/validação do comparador e 23 de integração, sem falhas ou testes ignorados.
* Release: 500 sementes × 1.500 comandos × RESP2/RESP3 = 1.500.000 comandos sequenciais;
  mais 20.000 em pipeline e 10.000 sobre valores comprimidos: **1.530.000 comandos gerados**,
  além de preparação e conferência do estado final. Debug: 100 sementes, 330.000 comandos gerados.
* TTL positivo passou a ser comparado numericamente, com tolerância de 100 ms mais o tempo da
  requisição/lote e arredondamento de 1 s nos comandos em segundos. `-1`/`-2` coincidem exatamente.
  Testes de prazo absoluto exigem o milissegundo exato. Estado final, bytes binários, TTL durante
  compressão e limpeza/reuso após FLUSHALL também são conferidos.
* Regressões do harness rejeitam carga com erros, contagem errada, replies truncados, misses,
  timeout de compressão, resultados CSV inválidos e diferença de contagem acima do limite em trânsito. `fmt`, clippy com `-D warnings`, doctests e
  ShellCheck passaram. Logs estão em [`benchmark-results/2026-10-09/tests`](benchmark-results/2026-10-09/tests).
* A checagem independente recalcula os 99 registros a partir dos JSON/CSV e amostras de memória.
  Gráficos e este documento são gerados dos mesmos resumos, evitando transcrição manual.
  O CI confere os registros publicados e exige que documento e gráficos coincidam com a regeneração.

## Correções e limites

As tabelas antigas não tinham saídas brutas preservadas e misturavam versões 0.2/0.3. Foram
substituídas por esta execução documentada. Não é possível certificar os valores históricos apenas
pelas tabelas. A carga mista antiga não pré-carregava todas as chaves; misses podiam variar durante
as rodadas. O cliente atual também usa sementes distintas por conexão, enquanto o script antigo
usava a configuração padrão de sementes do memtier. Seus percentuais não são diretamente comparáveis aos atuais, que exigem zero misses.

O tamanho médio antigo era calculado por linhas, incluindo CR e podendo contar caracteres Unicode
em vez de bytes. Agora usa o comprimento de cada bulk RESP. O timeout antigo de compressão não
invalidava a rodada; agora a cobertura integral é obrigatória. JSON gerado é identificado como sintético.

A conferência cruzada de GET considera respostas em trânsito no fim do teste com pipeline:
o servidor pode executar até conexões × pipeline leituras que o cliente não contabiliza ao encerrar.
Diferenças negativas ou acima desse limite invalidam a rodada; esse ajuste tem teste de regressão.

O antigo “pico de memória” era uma amostra depois da carga. O campo atual
`observed_post_load_max_bytes_per_key` é apenas o maior valor **observado depois da carga**, com
intervalos entre amostras; não inclui a carga e não estabelece um limite superior do processo.
O footprint do macOS tem arredondamento, e custos fixos de treino/dicionários pesam mais em bases pequenas.
O treino amostra dados em background, então o dicionário e o footprint podem variar mesmo com
o mesmo conjunto determinístico. No Linux o script mede RSS, uma métrica diferente: os valores não devem ser misturados com footprint.

`--threads 1` não impõe afinidade de CPU nem remove threads auxiliares. O cliente pode ser o gargalo,
a carga do sistema e a temperatura podem variar, e três rodadas não demonstram significância estatística.
A auditoria local cobre macOS/arm64. Linux, Docker, MSRV e audit de dependências não foram reexecutados
nesta sessão; a cobertura de CI deve ser consultada separadamente. Esses resultados não demonstram
escala em vários núcleos ou comportamento de dados de produção.

## Como reproduzir

```bash
CRABCACHE_REQUIRE_REDIS=1 CRABCACHE_DIFF_SEEDS=500 cargo test --release --all-targets
CRABCACHE_REQUIRE_REDIS=1 CRABCACHE_DIFF_SEEDS=100 cargo test --all-targets
python3 -m unittest discover -s scripts -p 'test_*.py' -v

# Os scripts compilam e sobem instâncias próprias, em portas locais livres.
REPEATS=3 SECS=10 scripts/bench-1cpu.sh
REPEATS=3 scripts/bench-memory.sh
REPEATS=3 SECS=10 scripts/bench-compression.sh 300000
REPEATS=3 scripts/bench.sh

# Conferir os artefatos publicados e regenerar documento/gráficos.
python3 scripts/validate-benchmark-results.py docs/benchmark-results/2026-10-09
python3 scripts/report-benchmarks.py
python3 scripts/gen-charts.py docs/img
```

`OUT_DIR` define um diretório novo para cada execução; o padrão fica em `target/benchmarks/`.
`SECS`, `REPEATS`, `KEYSPACE`, `N`, `LARGE_N` e `VALUE_SIZE` são configuráveis conforme os comentários
nos scripts. Para compressão, `SKIP_MEMORY=1` ou `SKIP_GET=1` escolhem a parte desejada e
`PACK_TIMEOUT` controla a espera. Essas execuções parciais não passam na validação do conjunto publicado,
que exige os 99 registros completos. Os arquivos RESP grandes são removidos depois da conferência;
o gerador determinístico, contagem, tamanho e hash ficam preservados.

`scripts/bench.sh crab-port redis-port` e `scripts/bench-1cpu.sh crab-port redis-port` também aceitam
servidores locais externos **vazios e descartáveis**. Essas instâncias recebem a carga e um FLUSHDB
após cada rodada. Para a comparação por thread, o CrabCache externo deve usar `--threads 1`.
