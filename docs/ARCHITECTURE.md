# Arquitetura

O CrabCache v2 é um único binário que fala o protocolo do Redis, em RESP2 ou RESP3 (via `HELLO 3`). Por isso `redis-cli`,
`redis-benchmark`, `memtier_benchmark` e as bibliotecas cliente do Redis funcionam sem alteração.

```
thread principal: accept + expiração ativa + evictor
  │  entrega cada conexão a uma thread de I/O (adaptativo)
  ▼
threads de I/O (1 por núcleo), cada uma com seu runtime tokio e seu kqueue/epoll
  └─ uma task por conexão
       lê → analisa todos os comandos completos → executa → acumula respostas → um write
                                   │
                                   ▼
                     Db: 2^k shards compartilhados, cada um um Mutex<Shard>
                       Shard: entradas em blocos + índice HashTable<u32> + min-heap de expiração
```

## Threads e conexões

* **Thread-per-core.** Cada thread de I/O roda um runtime tokio single-thread com seu próprio driver de
  eventos. Uma conexão é lida, executada e respondida na mesma thread, sem despertares entre threads.
  No runtime multi-thread usado antes (com roubo de tarefas), a eficiência por núcleo caía 27% com 2
  threads e 55% com 4.
* **Distribuição adaptativa.** A thread principal aceita conexões e entrega cada uma à thread ativa
  menos carregada. Só ficam ativas `ceil(conexões / io-conns-per-thread)` threads (padrão 32). Com carga
  leve, poucas threads bem ocupadas gastam menos CPU com despertares do que muitas threads ociosas.
  Medido: 156k rps com 1 thread contra 93k com 10, para 50 clientes sem pipeline.
* Uma conexão fica na mesma thread até fechar; ainda não há migração entre threads.

## Caminho de uma requisição

* `src/server.rs` lê para um `BytesMut`, analisa e executa **todos** os comandos completos do buffer e
  só então escreve. As respostas se acumulam num único buffer de saída e saem num único `write`
  (descarregado antes se passar de 64 KB). Com pipelining, isso agrupa muitas respostas por syscall.
* `src/protocol/parser.rs` é incremental: um multibulk parcial guarda posição e argumentos entre
  leituras, e uma linha inline incompleta lembra até onde já foi varrida. O parsing é linear nos bytes
  recebidos. Os limites seguem o Redis: linhas inline e cabeçalhos de 64 KB, bulk strings de 512 MB,
  1M de argumentos, mais um limite de buffer por cliente (1 GB por padrão). Erros de protocolo
  respondem `-ERR Protocol error: ...` e fecham a conexão.
* Os argumentos são fatias emprestadas do buffer de leitura; nada é copiado até um valor ser gravado.

## Armazenamento

* `Db` faz hash das chaves com `ahash` e semente aleatória por processo (resistente a hash flooding).
  O shard é escolhido pelos bits 40+ do hash, independentes dos bits que o `hashbrown` usa para
  buckets e tags. O padrão é 64 shards por thread, cada mutex na sua própria linha de cache de 128 bytes.
* **`Entry` tem 16 bytes**: um ponteiro para uma única alocação, o tamanho da chave (o bit mais alto
  indica se há TTL) e 32 bits de metadados de eviction. A alocação contém
  `[tamanho do valor: u32][expiração: u64, só se houver TTL][chave][valor]`. Chaves sem TTL não pagam
  pela expiração. Fora as duas chamadas de configuração do mimalloc em `src/main.rs`, este é o único
  código `unsafe` do projeto, isolado em `src/store/entry.rs`.
* **Entradas em blocos fixos de 256** (`src/store/chunked.rs`, 4 KB cada). Um `Vec` comum realoca e
  copia ao crescer, deixando folga e buffers antigos; blocos fixos nunca se movem. A posição continua
  O(1), então a amostragem aleatória da eviction funciona. A tabela hash guarda posições `u32`, e
  remoções usam `swap_remove`, corrigindo o índice da entrada movida.
* Os valores são montados fora do lock do shard e trocados lá dentro.
* O alocador é o mimalloc, configurado na inicialização para devolver páginas livres ao sistema
  imediatamente (`purge_delay = 0`). Com o padrão (1 s), uma thread ociosa nunca devolvia a memória
  liberada quando o índice crescia; isso custava ~14 bytes por chave.

Custo medido por chave com valores de 100 B: 159 bytes, contra 184 do Redis 8.10 (ver `BENCHMARKS.md`).

## Expiração

* Preguiçosa: toda busca apaga uma entrada expirada e a reporta como inexistente.
* Ativa: cada shard mantém um min-heap de `(prazo, hash da chave)`. Uma task em background roda a cada
  100 ms e remove os prazos vencidos, com trabalho limitado por shard em cada passada. Itens obsoletos
  do heap (TTL alterado ou chave apagada) são ignorados, e o heap é reconstruído quando passa do dobro
  do número de chaves com TTL.

## Limite de memória e eviction

* A memória é estimada por entrada (`alocação + 36 bytes`), contada por shard e publicada num contador
  global quando o lock do shard é liberado.
* `noeviction` rejeita escritas com `-OOM` acima de `maxmemory`, como no Redis.
* `allkeys-lru`, `allkeys-lfu` e `allkeys-random` removem do próprio shard do escritor, amostrando
  `maxmemory-samples` entradas aleatórias. O LFU usa o contador logarítmico com decaimento do Redis.
  Se um shard sozinho não cobrir o excesso, um evictor em background varre todos os shards.

## Testes de compatibilidade

`tests/differential.rs` envia sequências aleatórias de comandos (válidos, inválidos, com overflow,
com aridade errada) a um Redis real e ao CrabCache, e exige respostas idênticas byte a byte. Respostas
que dependem do tempo (TTL restante) são comparadas por classe, e a saída de `KEYS` é ordenada. O CI
roda contra o Redis 8 e falha se o Redis não estiver disponível, em vez de pular o teste.

Entradas que o gerador evita de propósito:

* Expiração relativa (`EX`/`PX`) em que `agora + n` estoura um i64. O Redis detecta esse caso por
  overflow de inteiro com sinal, que é comportamento indefinido em C. O build Linux (gcc) responde
  `ERR invalid expire time`, como pretendido, e o build do Homebrew no macOS (clang) aceita o comando.
  O CrabCache segue o comportamento pretendido, coberto por teste de integração.
* `GT`/`LT` com expirações relativas: aplicar o mesmo TTL duas vezes no mesmo milissegundo produz
  prazos iguais, então a resposta dependeria do tempo.

O teste passou com 500 seeds (750 mil comandos) contra o Redis 8.10.2 tanto no Linux (imagem oficial)
quanto no macOS (Homebrew).

## Lacunas conhecidas

* Só strings: sem listas, hashes, sets, sorted sets e streams.
* Sem MULTI/EXEC, WATCH, Lua, pub/sub, usuários ACL, persistência ou replicação.
* `SCAN` pode pular uma chave que uma remoção concorrente moveu para uma posição já visitada.
* `APPEND` copia o valor (O(n) por chamada).
* Conexões não migram entre threads; uma thread com poucas conexões muito ativas pode saturar
  enquanto outras ficam ociosas.
