# Graph Report - crabcache  (2026-10-19)

## Corpus Check
- 144 files · ~280,618 words
- Verdict: corpus is large enough that graph structure adds value.
- Unclassified: 43 file(s) not represented in the graph (top: .stdout 24, .log 8, (none) 5)

## Summary
- 830 nodes · 1840 edges · 38 communities (29 shown, 9 thin omitted)
- Extraction: 96% EXTRACTED · 4% INFERRED · 0% AMBIGUOUS · INFERRED: 68 edges (avg confidence: 0.86)
- Token cost: 0 input · 0 output

## Graph Freshness
- Built from commit: `baf39153`
- Run `git rev-parse HEAD` and compare to check if the graph is stale.
- Run `graphify update .` after code changes (no API cost).

## Community Hubs (Navigation)
- Execução de comandos Redis
- Banco e políticas de memória
- Compressão e dicionários CrabPack
- Shards e índices de armazenamento
- Servidor e conexões TCP
- Parser RESP e configuração
- Layout compacto de entradas
- Arquitetura e evidências publicadas
- Testes diferenciais com Redis
- Validação estatística dos benchmarks
- Testes de integração
- Infraestrutura dos testes
- Custos de leitura e memória
- Blocos fixos de entradas
- Geração de dados sintéticos
- Execução dos benchmarks
- Cliente RESP e smoke tests
- Relatório de análise de leituras
- Serialização de respostas RESP
- Medição de memória física
- Medição dos custos de leitura
- Geração de gráficos SVG
- Dados dos testes CrabPack
- Argumentos emprestados do protocolo
- Objeto pronto no cliente
- Resumo de latência e CPU
- Identidade visual CrabCache
- Decomposição do tempo de leitura
- Memória de JSON comprimido
- Comparação de memória por chave
- Throughput com uma thread
- Script principal de benchmark
- Script de benchmark monothread
- Script de benchmark CrabPack
- Script de benchmark de memória
- Versão da biblioteca
- Metadados do pacote

## God Nodes (most connected - your core abstractions)
1. `Ctx` - 70 edges
2. `Args` - 65 edges
3. `Shard` - 48 edges
4. `Entry` - 44 edges
5. `Db` - 44 edges
6. `require()` - 33 edges
7. `Codec` - 29 edges
8. `Bench` - 21 edges
9. `Policy` - 19 edges
10. `ShardGuard` - 17 edges

## Surprising Connections (you probably didn't know these)
- `random_command()` --calls--> `v()`  [INFERRED]
  tests/differential.rs → src/protocol/parser.rs
- `Sixteen byte entry layout` --references--> `Entry`  [EXTRACTED]
  docs/ARCHITECTURE.md → src/store/entry.rs
- `Mutex protected sharded store` --references--> `Db`  [EXTRACTED]
  docs/ARCHITECTURE.md → src/store/mod.rs
- `audit()` --indirect_call--> `rows()`  [INFERRED]
  docs/analises/leituras-crabpack-2026-10-11/graficos.py → scripts/gen-charts.py
- `values_from()` --calls--> `read_resp()`  [INFERRED]
  docs/analises/leituras-crabpack-2026-10-11/medir.py → scripts/benchmark.py

## Import Cycles
- 1-file cycle: `src/server.rs -> src/server.rs`
- 2-file cycle: `src/store/mod.rs -> src/store/shard.rs -> src/store/mod.rs`

## Hyperedges (group relationships)
- **Compact storage memory design** — docs_architecture_compact_entry, docs_architecture_chunked_entries, docs_architecture_mimalloc_purge [EXTRACTED 1.00]
- **Recorded evidence to verified published artifacts** — docs_benchmark_results_2026_10_09_readme_evidence, docs_benchmarks_audit, _github_workflows_ci_benchmark_consistency [EXTRACTED 1.00]
- **Complementary read cost and memory views** — docs_analises_leituras_crabpack_2026_10_11_comparacao_leituras_chart, docs_analises_leituras_crabpack_2026_10_11_memoria_apos_treino_chart, docs_analises_leituras_crabpack_2026_10_11_objeto_pronto_chart, docs_analises_leituras_crabpack_2026_10_11_relatorio_read_analysis [INFERRED 0.95]

## Communities (38 total, 9 thin omitted)

### Community 0 - "Execução de comandos Redis"
Cohesion: 0.07
Nodes (87): count_existing(), dbsize(), del(), exists(), expire(), expire_generic(), expire_of(), expireat() (+79 more)

### Community 1 - "Banco e políticas de memória"
Cohesion: 0.07
Nodes (14): Db, eviction_keeps_memory_near_limit(), memory_accounting_tracks_writes_and_flush(), noeviction_rejects_writes_over_limit(), OutOfMemory, PackCursor, Padded, Policy (+6 more)

### Community 2 - "Compressão e dicionários CrabPack"
Cohesion: 0.06
Nodes (27): Codec, CodecStats, compress_with(), compressor(), configure_compressor(), Corrupt, decompressor(), Dict (+19 more)

### Community 3 - "Shards e índices de armazenamento"
Cohesion: 0.08
Nodes (24): Chunked, Clock, clock(), expiry_heap_is_compacted(), Footprint, get(), idle_secs(), insert_overwrite_remove_keep_index_consistent() (+16 more)

### Community 4 - "Servidor e conexões TCP"
Cohesion: 0.06
Nodes (19): arity_ok(), execute(), Session, Shared, Stats, unknown_command(), Config, ClientSlot (+11 more)

### Community 5 - "Parser RESP e configuração"
Cohesion: 0.07
Nodes (27): parse_mem_arg(), parse_policy_arg(), configure_allocator(), GLOBAL, main(), MI_OPTION_PURGE_DELAY, shutdown_signal(), args_of() (+19 more)

### Community 6 - "Layout compacto de entradas"
Cohesion: 0.11
Nodes (12): accessors_and_mutation(), Entry, ENTRY_OVERHEAD, EXPIRE_BYTES, KEY_LEN_MASK, LEN_BYTES, memory_usage_counts_optional_fields_only_when_present(), Packed (+4 more)

### Community 7 - "Arquitetura e evidências publicadas"
Cohesion: 0.06
Nodes (28): CI compatibility and artifact validation, Mandatory Redis differential tests, Authenticated Docker and CrabPack smoke test, Rust 1.85 minimum supported version, Test gated multiarchitecture Docker publishing, Version tag release pipeline, CrabCache 0.3.0 CrabPack release, CrabPack six metric read comparison chart (+20 more)

### Community 8 - "Testes diferenciais com Redis"
Cohesion: 0.12
Nodes (20): array_items(), commands_match_redis(), compare_state(), connect_pair(), connect_redis(), far_future_secs(), INTS, KEYS (+12 more)

### Community 9 - "Validação estatística dos benchmarks"
Cohesion: 0.10
Nodes (10): aggregate(), close(), open_dataset(), plot_pairs(), selected(), parse_memtier(), parse_redis_benchmark(), validate_get_counts() (+2 more)

### Community 11 - "Infraestrutura dos testes"
Cohesion: 0.12
Nodes (5): Raw, reply_len(), TestServer, wait_for_packed(), crabpack_compresses_idle_values_transparently()

### Community 12 - "Custos de leitura e memória"
Cohesion: 0.09
Nodes (19): Synthetic Loopback Benchmark Scope, CrabPack Read Cost Comparison, CrabPack ligado e desligado, Latência de GET individual, GET sob carga concorrente, RAM por chave após treino, Metodologia e limites do benchmark, GET e objeto pronto em Python (+11 more)

### Community 13 - "Blocos fixos de entradas"
Cohesion: 0.17
Nodes (4): CHUNK, Chunked<T>, drains_to_empty(), matches_vec_semantics()

### Community 14 - "Geração de dados sintéticos"
Cohesion: 0.18
Nodes (11): ADJ, api(), CATS, FIRST, LAST, main(), product(), Rng (+3 more)

### Community 15 - "Execução dos benchmarks"
Cohesion: 0.35
Nodes (3): Bench, main(), require()

### Community 16 - "Cliente RESP e smoke tests"
Cohesion: 0.22
Nodes (6): audit(), Client, dataset_info(), encode(), read_resp(), main()

### Community 17 - "Relatório de análise de leituras"
Cohesion: 0.24
Nodes (7): Analysis, blocks(), chart(), close(), main(), pair(), pt()

### Community 19 - "Serialização de respostas RESP"
Cohesion: 0.20
Nodes (10): array(), bulk(), bulk_int(), error(), invalid_expire(), map(), not_integer(), syntax_error() (+2 more)

### Community 20 - "Medição de memória física"
Cohesion: 0.29
Nodes (3): main(), cpu_seconds(), physical_bytes()

### Community 21 - "Medição dos custos de leitura"
Cohesion: 0.33
Nodes (7): application_readback(), compression_ready(), large_api(), main(), make_datasets(), measure_get(), values_from()

### Community 22 - "Geração de gráficos SVG"
Cohesion: 0.22
Nodes (4): bar(), chart(), rows(), value()

### Community 23 - "Dados dos testes CrabPack"
Cohesion: 0.32
Nodes (3): Rng, session_json(), packed_binary_values_keep_expiry_and_survive_flush_and_reload()

### Community 25 - "Objeto pronto no cliente"
Cohesion: 0.50
Nodes (5): Cliente: conferir e interpretar JSON, Tempos até objeto pronto com CrabPack, GET: rede e servidor, Metodologia e variação do objeto pronto, Do GET ao objeto utilizável

### Community 26 - "Resumo de latência e CPU"
Cohesion: 0.60
Nodes (4): CrabPack README read latency and CPU summary, Individual GET mean latency comparison, October 11 short synthetic loopback read workload, Server CPU cost per GET under concurrency

### Community 27 - "Identidade visual CrabCache"
Cohesion: 0.67
Nodes (4): Orange Crab Mascot, CrabCache Logo, CrabCache Wordmark, Stacked Storage Symbol

### Community 28 - "Decomposição do tempo de leitura"
Cohesion: 0.67
Nodes (4): Client JSON Validation And Interpretation, GET To Application Ready Object, Ready Object Latency By Workload, Server Side CrabPack Decompression

### Community 29 - "Memória de JSON comprimido"
Cohesion: 0.50
Nodes (4): API JSON Memory Per Key, Synthetic JSON Memory Per Key Benchmark, Product JSON Memory Per Key, Session JSON Memory Per Key

### Community 30 - "Comparação de memória por chave"
Cohesion: 0.50
Nodes (4): CrabCache 0.3, Memória por chave: Redis e CrabCache, Metodologia de memória por chave, Redis 8.10

### Community 31 - "Throughput com uma thread"
Cohesion: 0.67
Nodes (3): Throughput With Pipeline 16, Single I/O Thread Throughput Benchmark, Throughput Without Pipelining

## Knowledge Gaps
- **50 isolated node(s):** `crabcache`, `Ex`, `Px`, `ExAt`, `PxAt` (+45 more)
  These have ≤1 connection - possible missing edges or undocumented components. (Counts symbols only; 258 node(s) total have ≤1 connection when file, concept and rationale nodes are included.)
- **9 thin communities (<3 nodes) omitted from report** — run `graphify query` to explore isolated nodes.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **Why does `Db` connect `Banco e políticas de memória` to `Execução de comandos Redis`, `Compressão e dicionários CrabPack`, `Shards e índices de armazenamento`, `Servidor e conexões TCP`, `Arquitetura e evidências publicadas`?**
  _High betweenness centrality (0.098) - this node is a cross-community bridge._
- **What connects `crabcache`, `Ex`, `Px` to the rest of the system?**
  _50 weakly-connected nodes found - possible documentation gaps or missing edges._
- **Should `Execução de comandos Redis` be split into smaller, more focused modules?**
  _Cohesion score 0.06666666666666667 - nodes in this community are weakly interconnected._
- **Why does `Args` connect `Execução de comandos Redis` to `Servidor e conexões TCP`, `Parser RESP e configuração`?**
  _High betweenness centrality (0.092) - this node is a cross-community bridge._
- **Should `Banco e políticas de memória` be split into smaller, more focused modules?**
  _Cohesion score 0.06605222734254992 - nodes in this community are weakly interconnected._
- **Why does `Entry` connect `Layout compacto de entradas` to `Execução de comandos Redis`, `Compressão e dicionários CrabPack`, `Shards e índices de armazenamento`, `Arquitetura e evidências publicadas`?**
  _High betweenness centrality (0.085) - this node is a cross-community bridge._
- **Should `Compressão e dicionários CrabPack` be split into smaller, more focused modules?**
  _Cohesion score 0.055299539170506916 - nodes in this community are weakly interconnected._