# CrabPack adaptativo: implementação e validação local

Branch `perf/crabpack-adaptive`; implementação `6df844c31a50f7b807ba9d7c3633b2b3860fda7f`.
A comparação usa a otimização anterior `a1d24ec` como baseline, não a branch original
nem o Redis. Os dois executáveis nativos foram recompilados com o mesmo Rust 1.99.0.
Os artefatos desta campanha usam o identificador `2026-10-14`.

## Mudanças

- **Dicionários por custo total estimado:** testa tamanhos de 4, 8 e 16 KiB nos níveis
  Zstd 1 e 3. Treina com 80% das amostras e avalia nos 20% restantes. Inclui o cabeçalho
  dos valores compactados e as estruturas preparadas de compressão/descompressão.
  Exige ganho de payload de pelo menos 1,25x e economia estimada sobre a população
  observada de amostras. Isso não estima toda a população futura do prefixo.
- **Leitura com uma cópia a menos:** descompacta valores completos diretamente no
  buffer da resposta RESP. A leitura parcial de GETRANGE continua usando scratch.
  Mantém validação do tamanho e desfaz a escrita no buffer em caso de erro. Os
  snapshots de valores grandes continuam liberando o lock antes de descompactar.
- **TTL mínimo:** evita amostrar ou compactar valores com menos de dois segundos
  restantes por padrão; revalida a elegibilidade ao instalar o resultado.
- **Manutenção adaptativa:** mede o tempo de execução de um comando a cada 64 por
  conexão. Uma amostra de pelo menos 1 ms sinaliza pressão durante um segundo.
  Nesse intervalo, pula ticks alternados, adia novas amostragens/treinamentos e
  reduz o orçamento temporal de compactação pela metade, com piso de 1 ms.
  Com used_memory a partir de 75% de maxmemory, mantém os ticks e o treinamento
  para evitar inanição. A medição exclui socket e espera anterior à execução;
  não é CPU nem latência ponta a ponta. Não interrompe treinamento já iniciado.
- **Promoção de valores quentes, opcional:** em allkeys-lfu, usa o contador LFU
  existente para transformar valores muito acessados em entradas sem compressão.
  O orçamento reserva memória antes da troca, preserva TTL e não provoca eviction
  nem erro OOM no GET. Fica desligada por padrão.

Os dicionários aceitos continuam residentes até o fim do processo, com limite de
64; não há rotação ou retreinamento de dicionários já aceitos. Entry permanece com
16 bytes. As contagens de memória de entradas não representam o RSS do processo.

## Controles

Disponíveis em CLI, ambiente, CONFIG GET/SET e INFO:

| CONFIG / CLI | Ambiente | Padrão |
|---|---|---|
| compression-adaptive-dicts | CRABCACHE_COMPRESSION_ADAPTIVE_DICTS | yes / true |
| compression-adaptive-maintenance | CRABCACHE_COMPRESSION_ADAPTIVE_MAINTENANCE | yes / true |
| compression-min-ttl | CRABCACHE_COMPRESSION_MIN_TTL | 2 segundos |
| compression-hot-maxmemory | CRABCACHE_COMPRESSION_HOT_MAXMEMORY | 0, desativado |

Os booleanos novos da CLI recebem valor explícito, por exemplo
`--compression-adaptive-dicts false`. CONFIG usa yes/no.
Desligar adaptive-dicts usa a seleção anterior de 16 KiB/nível 3 para novos
prefixos; não substitui os dicionários existentes. Para uma comparação limpa,
reinicie com uma base nova. min-ttl=0 desativa o filtro de TTL.

A promoção LFU divide o orçamento pelos shards e limita o acompanhamento a 64
entradas por shard. Contabiliza o tamanho residente inteiro e o acompanhamento,
não apenas o crescimento. Reduzir ou zerar o orçamento impede novas promoções;
as entradas já promovidas continuam sem compressão até escrita, remoção ou nova
compactação por inatividade. Use min-idle positivo para evitar alternância entre
compactação e promoção. Exemplo de experimento separado:

```text
CONFIG SET compression-min-idle 5
CONFIG SET compression-hot-maxmemory 4mb
```

INFO acrescenta `compression_hot_resident_bytes`, `compression_hot_promotions`,
`compression_short_ttl_skips`, `compression_maintenance_backoffs` e os parâmetros
novos. Cada `dictN` também informa raw_bytes, level e prepared_bytes.

## Resultados locais

### Dataset do playground: 4.500 chaves

Três repetições por versão, ordem alternada, processo novo, loopback no macOS
Apple Silicon, um worker e 64 shards, min-idle=0. O cliente executou cache-bench
1500 60. As seis rodadas terminaram com todas as chaves compactadas e verificadas.
Hash do dataset: `7d3bd8853de4c2e1081991b7df736d610a4200ca504447e2580864ffe8f5f1b1`.

Medianas em bytes, exceto quando indicado:

| Métrica | a1d24ec | 6df844c | Variação |
|---|---:|---:|---:|
| Payload original | 2.926.490 | 2.926.490 | igual |
| Payload armazenado | 232.461 | 227.828 | -2,0% |
| Memória contabilizada das entradas | 569.961 | 565.328 | -0,8% |
| Estruturas dos dicionários preparados | 1.404.927 | 323.607 | -77,0% |
| Entradas + dicionários preparados | 1.974.888 | 888.935 | -55,0% |
| Razão do payload | 12,59x | 12,85x | +2,0% |
| Tempo acumulado de treinamento, ms | 46,827 | 93,176 | aproximadamente 2x |

A seleção adaptativa escolheu 16 KiB/nível 1 nos três prefixos. Os 55% são redução
na soma de componentes conhecidos, **não uma redução de RSS de 55%**. Essa soma
não inclui todas as alocações do servidor. O treinamento mede tempo decorrido,
não CPU; procurar candidatos custa mais durante o aquecimento.

### GET sob concorrência

Três repetições, fases de quatro segundos, 48 clientes memtier, pipeline=1,
quatro workers e um shard para enfatizar contenção. Dois datasets: 10.000 JSONs
de aproximadamente 264 bytes; 128 JSONs de aproximadamente 133 KB. O caso grande
pré-treina com 2.000 objetos pequenos, mantidos fora do intervalo medido de GETs.
A fase packed começa com 100% das chaves compactadas. Todos os valores foram
verificados, sem misses ou erros nos GETs medidos.

| Dataset/fase | GET médio, ms: antes → depois | p99 mediano, ms: antes → depois | CPU/GET, µs: antes → depois |
|---|---:|---:|---:|
| Pequenos / transição | 0,3265 → 0,3253 | 0,463 → 0,463 | 14,43 → 14,39 |
| Pequenos / packed | 0,3258 → 0,3252 | 0,471 → 0,463 | 14,92 → 14,88 |
| Grandes / transição | 1,6715 → 1,4566 | 2,831 → 2,751 | 49,05 → 47,25 |
| Grandes / packed | 1,4808 → 1,3818 | 2,847 → 2,639 | 69,70 → 62,40 |

Nos grandes já compactados: GET médio -6,7%, p99 mediano -7,3%, CPU/GET -10,5%;
throughput mediano 32.450 → 34.587 GET/s. Nos pequenos, o resultado é próximo da
paridade. As médias e CPU/GET usam totais ponderados por operações; os p99 são
medianas dos percentis de cada rodada, não um percentil agrupado.

A transição tem trajetórias de cobertura diferentes e não isola descompressão.
O RSS mediano após a fase packed grande **subiu** de 26.984.448 para 28.950.528
bytes; não houve benefício geral de RSS demonstrado. A promoção quente ficou
desligada e não houve backoffs registrados. Portanto, os ganhos medidos não
validam desempenho dessas duas políticas. Elas têm validação funcional.

São medições locais curtas, sem quota de CPU do Fly. Não demonstram eliminação
de picos de segundos em produção nem superioridade sobre Redis. A próxima rodada
no Fly deve repetir a mesma carga e registrar CPU/throttling, RSS, cobertura,
INFO antes/depois e os resultados brutos do cliente, separando aquecimento e
estado já compactado. Compare promoção quente em uma rodada adicional.

## Evidências e reprodução

- [Resumo numérico](benchmark-results/2026-10-14-crabpack-adaptive-comparison.json).
- [Footprint: resumo, hashes e seis JSONs](benchmark-results/2026-10-14-crabpack-adaptive-footprint/summary.json).
- [Latência: resumo e 24 registros](benchmark-results/2026-10-14-crabpack-latency-adaptive/summary.json).
- [Testes release](benchmark-results/2026-10-14-crabpack-adaptive-checks/tests.log):
  84 aprovados, incluindo diferencial com Redis, protocolos, concorrência,
  corrupção de payload, TTL e orçamento das promoções.
- fmt, clippy all-targets com warnings negados e doc-tests passaram. Build
  locked all-targets com Rust 1.85 passou, mantendo o MSRV.
- [Validação Linux](benchmark-results/2026-10-14-crabpack-adaptive-checks/linux-smoke.log):
  imagem do pacote linux/amd64, 1 CPU e 512 MiB; AUTH, 2.000 valores binários
  compactados, valor de 141.317 bytes, GET/MGET/GETRANGE/GETDEL, TTL, APPEND e
  promoção LFU com limites. Executado via emulação no Docker Desktop; teste
  funcional, sem alegação de performance do Fly.

Para conferir os dados existentes sem rodar carga:

```sh
python3 scripts/bench-crabpack-footprint.py --check docs/benchmark-results/2026-10-14-crabpack-adaptive-footprint
python3 scripts/bench-crabpack-latency.py --check docs/benchmark-results/2026-10-14-crabpack-latency-adaptive
```

Para novas rodadas, use diretórios de saída novos. No layout atual, a API do
playground fica no diretório pai deste worktree; compile-a nativamente em
`../playground/realistic_cache_fly/api` com `go build -o /tmp/crabpack-api .`.
A partir deste worktree, com ambos os servidores compilados em release:

```sh
python3 scripts/bench-crabpack-footprint.py --baseline ../crabcache-crabpack-perf/target/release/crabcache --candidate target/release/crabcache --api /tmp/crabpack-api --repeats 3 --output /tmp/crabpack-footprint-novo
python3 scripts/bench-crabpack-latency.py --baseline ../crabcache-crabpack-perf/target/release/crabcache --candidate target/release/crabcache --repeats 3 --seconds 4 --output /tmp/crabpack-latency-novo
```

## Pacote de teste no Fly

O pacote externo ao Git fica em
`../builds/crabpack-adaptive-6df844c-linux-amd64/`, com binário, Dockerfile de
runtime, fly.toml, checksums, proveniência e instruções. A imagem de origem e a
imagem construída a partir do pacote passaram na validação funcional. A
configuração Fly foi validada; deploy não foi executado.
