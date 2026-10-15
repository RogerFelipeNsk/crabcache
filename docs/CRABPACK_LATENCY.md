# Branch de testes: perf/crabpack-latency

A implementação está no commit `24a2075`. A branch parte de `feat/crabpack`, commit `34438c2`,
e foi implementada no worktree
`crabcache-crabpack-perf`. As alterações que já existiam em `crabcache/` foram preservadas.
As mudanças buscam reduzir contenção e bursts do CrabPack, com regressões de integridade e
medições locais antes/depois. A versão do binário continua 0.3.0: identificar o experimento pelo
commit/tag e hash do binário, não apenas pelo número da versão.

## Comportamento implementado

- O relógio de acesso do CrabPack tem resolução de um segundo e independe de LRU/LFU.
  A virada do minuto não faz uma chave parecer ociosa por 60 s. Entradas mantêm 16 bytes;
  o relógio ocupa uma tabela lateral de aproximadamente **4 bytes por chave**, alocada por shard
  quando a compressão é habilitada. Esse custo entra em `used_memory` e `MEMORY USAGE`.
- Antes de instalar um valor compactado, revalidar acesso, conteúdo, chave, posição e expiração.
  Um GET ou uma escrita de conteúdo idêntico após a coleta invalida o candidato.
  Mudanças válidas de TTL e os contadores LFU atuais são preservados.
- Treinar **um dicionário por vez** e executar coleta/treino/compactação em um trabalho bloqueante
  separado do acceptor. Ticks atrasados são pulados, sem tentar recuperar todos em um burst.
- Por ciclo de 200 ms, limitar a compactação a **512 KiB de entrada** e uma janela cooperativa
  de **2 ms**, configuráveis. As aquisições do mutex coletam no máximo 128 posições por passo,
  encerrando normalmente ao copiar 64 KiB; um único valor maior pode ultrapassar esse tamanho
  de passo, respeitando o limite do ciclo. Shards inteiramente compactados não são varridos.
- GET/MGET/GETRANGE de valores compactados de pelo menos **64 KiB originais** usam uma cópia
  dos bytes comprimidos e liberam o mutex antes de descomprimir/copiar a resposta. Valores
  pequenos conservam o caminho sem essa alocação. GETDEL remove sob lock e decodifica o valor
  removido após liberar o lock, mantendo a propriedade de consumo único.
- Admitir compactação apenas se o ganho pagar o header Packed de 8 bytes e poupar mais de
  1/8 do valor original. Um valor pequeno não cresce em memória por causa do header.
- Limitar a coleta a 64 grupos pendentes/dicionários no total e fragmentos de 16 KiB por amostra.
  Cada chave contribui uma vez por lote, evitando repetir a mesma chave entre treino e holdout.
  A amostra troca de dono sem uma segunda cópia do valor e não carrega a chave completa.
  Grupos pequenos podem levar mais tempo para treinar ou não atingir o mínimo de amostras.
- Expor no INFO custos de relógios, amostras, estruturas preparadas dos dicionários, treino em
  andamento e contadores de trabalho, candidatos descartados e valores acima do limite.

O deadline de 2 ms é verificado **entre** valores; não interrompe uma chamada zstd já iniciada.
O treino também não tem preempção por esse deadline: sua concorrência e entrada são limitadas.
Valores cuja chave+valor ultrapassam o limite de bytes do ciclo continuam plain; o limite pode ser
ampliado para arquivos maiores. As leituras devolvem os bytes originais em todas as condições.
Ao desabilitar a compressão, relógios já alocados continuam sendo atualizados; dicionários/valores
compactados seguem legíveis. FLUSHALL libera as entradas e o armazenamento dos relógios, mantendo
os dicionários, como antes. `used_memory` continua sendo contabilização, não RSS físico.

`compression_training_us` e `compression_pack_us` medem tempo decorrido do job, incluindo espera;
não são tempo de CPU. `compression_oversized_skips` conta tentativas de varredura, não chaves únicas.
Os campos de amostras/dicionários são estimativas de componentes, sem todos os custos do allocator.

## Configuração para o playground

| CLI / CONFIG | Variável de ambiente | Padrão |
|---|---|---|
| `--compression-max-bytes-per-tick` / `compression-max-bytes-per-tick` | `CRABCACHE_COMPRESSION_MAX_BYTES_PER_TICK` | `512kb` |
| `--compression-time-budget-ms` / `compression-time-budget-ms` | `CRABCACHE_COMPRESSION_TIME_BUDGET_MS` | `2` (1–1000) |

`compression-min-idle` e `compression-min-size` continuam configuráveis, inclusive via CONFIG.
Não foram alterados os padrões de 60 s/64 B; o playground pode manter seus overrides de 5 s/40 B.
Para valores maiores que 512 KiB, experimentar `2mb` de orçamento, acompanhando os tempos e CPU.
Como há custo de relógio por chave, habilitar o modo em um cache cheio exige essa folga adicional
ou eviction/compactação para voltar ao limite de dados armazenados.

Após revisar a branch, este comando faz deploy no cache do playground **quando executado**,
com contexto de build apontando para este worktree. A partir da pasta `CrabCache/`:

```bash
make -C playground/realistic_cache_fly/cache deploy-source \
  CRAB_SRC="$PWD/crabcache-crabpack-perf"
```

O protocolo e a API do playground permanecem compatíveis. O deploy não foi executado nesta tarefa.
Para comparar, usar imagens/commits identificáveis de baseline e candidato, repetir as rodadas,
registrar INFO/logs/CPU quota/throttling e separar GET direto de latência da API/Mongo/locks.

## Evidência local

A [medição completa](benchmark-results/2026-10-14-crabpack-latency-000923/summary.json) preserva
24 registros: duas versões × dois conjuntos × duas fases × três repetições de 4 s, com a ordem
invertida na segunda repetição. São 10.000 JSONs de 263,67 B e 128 JSONs de 133.063,09 B em média.
A segunda fase só inicia depois de 100% de compactação. Todas as chaves medidas foram conferidas
byte a byte após cada fase; não houve misses nem erros de conexão/comando no memtier.

Média de GET e CPU por GET calculadas pela soma de latências/CPU dividida pela soma das contagens;
p99 e throughput na tabela são medianas das três rodadas, não percentis globais agrupados.

| Conjunto / fase | GET médio baseline → branch (ms) | p99 baseline → branch (ms) | CPU baseline → branch (µs/GET) |
|---|---|---|---|
| JSON pequeno / transição | 0,3335 → 0,3218 | 0,519 → 0,447 | 14,545 → 14,404 |
| JSON pequeno / compactado | 0,3262 → 0,3221 | 0,479 → 0,455 | 14,949 → 14,881 |
| JSON grande / transição | 2,3821 → 1,5713 | 4,575 → 2,943 | 60,424 → 48,448 |
| JSON grande / compactado | **2,3448 → 1,4840** | **4,127 → 3,023** | **59,071 → 70,298** |

No JSON grande já compactado, a média caiu **36,7%** e o p99 mediano **26,8%**. O throughput
mediano passou de 20.418 para 32.829 GET/s, aproximadamente **+60,8%**. O custo foi **+19,0% de CPU
por GET** nessa condição. Essa troca precisa ser avaliada na CPU compartilhada da Fly: menor
contenção sob concorrência local não garante menor cauda quando há throttling de CPU.
No conjunto pequeno, os números ficaram próximos; um ensaio exploratório anterior teve pequena
piora em algumas rodadas, portanto não afirmar um ganho geral para objetos pequenos.

O limite de trabalho prolonga a compactação. Na transição de JSONs pequenos, 100% foi observado em
3,37–3,57 s no baseline e 5,26–5,46 s na branch. Para os grandes, o baseline terminou em cerca de
0,21 s e a branch em 8,52–8,81 s. A transição mede essas trajetórias **diferentes de cobertura**;
a diferença de CPU nessa fase não é um custo de descompressão isolado.

Para o conjunto grande, ambos treinam antes em 2.000 objetos pequenos idênticos do mesmo prefixo,
que permanecem no cache e estão fora da faixa dos GETs medidos. Isso evita um problema da versão
anterior: a coleta de 256 KiB pode encerrar com menos de 10 amostras grandes e o treino é rejeitado.
A branch também corrige esse caso usando fragmentos limitados; há um teste específico com valores
grandes e nenhum dicionário preparado previamente. Os objetos de preparo estão preservados no ZIP.

Ambiente: macOS/Apple Silicon, loopback, quatro threads de I/O, quatro threads × 12 conexões memtier,
pipeline 1, **um shard** para enfatizar contenção no mutex. Cliente e servidor compartilham o host.
Há um observador INFO/ps a cada 200 ms nos dois casos; seu trabalho está incluído na CPU do processo.
É um ensaio de estresse curto, sem quota de CPU da Fly, sem afinidade de núcleo e sem tráfego HTTP,
TLS ou Mongo. Os valores de RSS estão preservados por rodada e na linha do tempo, mas são variáveis:
não há promessa de economia de RAM física. O relógio acrescenta bytes à contabilização interna.

Hashes dos binários medidos:

```text
baseline:  035de37f306b807938d9fa4f7188ed68252497ff0db05a3d980d82a287bf1c9c
candidate: 492acfa40818628b8b28e0193fae624b70acab50446cdfdb6f77baa5c99a1c97
```

O resumo contém também a referência do commit base e o hash do diff de código medido. Os conjuntos
RESP, JSONs originais do memtier, comandos, snapshots INFO, CPU/RSS, logs e linhas do tempo ficam
na mesma pasta. Resultados históricos de 09/10 e arquivos preexistentes de 11/10 foram preservados.

## Verificação e reprodução

Passaram 74 testes Rust em release, com Redis obrigatório e 100 seeds no ensaio diferencial,
`cargo fmt --check`, clippy com warnings tratados como erro, build de todos os targets
com Rust 1.85.0 (`--locked`) e doc-tests. Há 12 regressões novas
nos módulos internos e cinco cenários novos de integração: precisão LFU, revalidação de acesso/
escrita/TTL, snapshots com escritas concorrentes, limites de amostragem, ganho líquido,
orçamento/retomada, maxmemory e treinamento de valores grandes. Também passaram sete testes Python
existentes e a conferência dos 99 registros e relatórios/gráficos históricos.

```bash
# Neste worktree:
cargo build --release
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
CRABCACHE_REQUIRE_REDIS=1 CRABCACHE_DIFF_SEEDS=100 cargo test --release --all-targets
cargo test --doc

# Conferir os dados preservados; não inicia servidores:
python3 scripts/bench-crabpack-latency.py --check \
  docs/benchmark-results/2026-10-14-crabpack-latency-000923

# Nova medição, depois de compilar o baseline no outro checkout:
python3 scripts/bench-crabpack-latency.py \
  --baseline ../crabcache/target/release/crabcache \
  --candidate target/release/crabcache --repeats 3 --seconds 4
```

Cada execução cria uma pasta nova e encerra os processos locais ao final. Os números são evidência
dos binários identificados, não garantia de qualquer commit posterior ou da latência da API na Fly.
