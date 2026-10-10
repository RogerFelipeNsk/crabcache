#!/usr/bin/env python3
"""Generate the benchmark document from recorded per-run summaries."""
import json
from pathlib import Path
from statistics import median
import sys

root = Path(sys.argv[1]) if len(sys.argv) > 1 else Path('docs/benchmark-results/2026-10-09')
output = Path(sys.argv[2]) if len(sys.argv) > 2 else Path('docs/BENCHMARKS.md')
data = {kind: json.loads((root / kind / 'summary.json').read_text())
        for kind in ['core', 'memory', 'compression', 'default']}


def selected(kind, **filters):
    rows = [r for r in data[kind]['results'] if all(r.get(k) == v for k, v in filters.items())]
    if len(rows) != 3:
        raise ValueError(f'expected three runs for {kind}: {filters}')
    return rows


def metric(rows, key):
    values = [r[key] for r in rows]
    return median(values), min(values), max(values)


def ops(v):
    return f'{v / 1e6:.2f}M' if v >= 1e6 else f'{v / 1e3:.1f}k'


def spread(values, fmt=ops):
    m, low, high = values
    return f'{fmt(m)} [{fmt(low)}–{fmt(high)}]'


def number(v):
    return f'{v:.1f}'


def pct(baseline, candidate):
    return f'{(candidate / baseline - 1) * 100:+.1f}%'


meta = data['core']['metadata']
text = [f'''# Benchmarks

Auditoria local de **09/10/2026**. Este documento e os gráficos são gerados a partir dos resumos
em [`benchmark-results/2026-10-09`](benchmark-results/2026-10-09). Cada resumo acompanha os comandos,
saídas stdout/stderr e JSON/CSV das ferramentas. Valores das tabelas são **medianas de três rodadas**;
os colchetes mostram mínimo–máximo das rodadas. Essa faixa não é um intervalo de confiança.
Os JSON/CSV numéricos ficam acessíveis diretamente; comandos e logs estão nos `traces.zip` de cada
conjunto. A validação lê esses arquivos sem precisar extrair os ZIPs.

## Ambiente e método

* {meta['cpu_model']}, {meta['cpu_count']} CPUs lógicas, {meta['memory_bytes'] // 1024**3} GiB, `{meta['platform']}`.
* `{meta['redis']}`; persistência desativada (`--save '' --appendonly no`).
* `{meta['crabcache']}`, build release; hash do binário e commit-base estão nos resumos.
* `{meta['memtier'].splitlines()[0]}`. Cliente e servidor compartilham a máquina, por loopback.
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
|---|---|---|---|---|''']
for pipeline in [1, 16]:
    med = {}
    for server in ['redis', 'crabcache']:
        rows = selected('core', server=server, pipeline=pipeline)
        med[server] = metric(rows, 'ops_s')[0]
        text.append(f"| {pipeline} | {server} | {spread(metric(rows, 'ops_s'))} | {spread(metric(rows, 'mean_cpu_cores'), lambda v: f'{v:.2f}')} | {spread(metric(rows, 'ops_per_cpu_second'))} |")
    text.append(f"| {pipeline} | Diferença de throughput | **{pct(med['redis'], med['crabcache'])}** | | |")

text.append('''
## Configuração padrão

`scripts/bench.sh`: `redis-benchmark`, 100.000 chaves de 100 B; 200.000 pedidos por comando com uma
conexão e 2.000.000 com 50 conexões. SET e GET medidos separadamente. CrabCache usa a configuração
padrão de threads. Cada célula mostra throughput [mín–máx] e a mediana de p50/p99 em milissegundos.

| Conexões | Pipeline | Comando | Redis ops/s · p50/p99 ms | CrabCache ops/s · p50/p99 ms |
|---|---|---|---|---|''')
for clients, pipeline in [(1, 1), (50, 1), (50, 16), (50, 64)]:
    for command in ['SET', 'GET']:
        cells = []
        for server in ['redis', 'crabcache']:
            rows = [r[command] for r in selected('default', server=server, clients=clients, pipeline=pipeline)]
            cells.append(f"{spread(metric(rows, 'ops_s'))} · {metric(rows, 'p50_ms')[0]:.3f}/{metric(rows, 'p99_ms')[0]:.3f}")
        text.append(f"| {clients} | {pipeline} | {command} | {' | '.join(cells)} |")

text.append('''
## Memória

`scripts/bench-memory.sh`: chaves `key:NNNNNNNNNNNN`, 1.000.000 de chaves para valores de 10/100 B,
300.000 para valores de 1000 B. Valores fixos preenchidos com `x`; compressão desativada.
Memória incremental por chave = (footprint final − footprint vazio) / N. O baseline contém custos
fixos do processo. Depois da carga, espera de 2 s e mediana de três amostras de footprint; em seguida,
todos os valores são relidos e comparados byte a byte.

| Valor | Chaves | Redis B/chave [mín–máx] | CrabCache B/chave [mín–máx] | Diferença |
|---|---|---|---|---|''')
for size in [10, 100, 1000]:
    redis = selected('memory', server='redis', value_size=size)
    crab = selected('memory', server='crabcache', value_size=size)
    r, c = metric(redis, 'bytes_per_key'), metric(crab, 'bytes_per_key')
    text.append(f"| {size} B | {redis[0]['keys']:,} | {spread(r, number)} | {spread(c, number)} | {pct(r[0], c[0])} |")

text.append('''
## CrabPack: JSON sintético

`scripts/bench-compression.sh`: `examples/dataset.rs` gera JSON sintético determinístico de sessões,
produtos e respostas de API. São 300.000 chaves por conjunto em cada instância. CrabPack usa
`--compression --compression-min-idle 0`; **100%** das chaves devem estar comprimidas antes da medição,
ou a rodada falha. Após medir memória, todas as 300.000 chaves são relidas e comparadas ao conjunto
original. A taxa é bytes originais / bytes comprimidos do payload, não a razão do footprint total.

| Conjunto | Média do valor | Redis B/chave [mín–máx] | CrabCache B/chave [mín–máx] | CrabPack B/chave [mín–máx] | Pack vs Redis | Taxa do payload |
|---|---|---|---|---|---|---|''')
for kind in ['session', 'product', 'api']:
    rows = {server: selected('compression', mode='memory', dataset=kind, server=server)
            for server in ['redis', 'crabcache', 'crabpack']}
    values = {server: metric(rr, 'bytes_per_key') for server, rr in rows.items()}
    mean = rows['redis'][0]['mean_value_bytes']
    cells = [spread(values[server], number) for server in ['redis', 'crabcache', 'crabpack']]
    ratio = metric(rows['crabpack'], 'compression_ratio')[0]
    text.append(f"| {kind} | {mean:.2f} B | {' | '.join(cells)} | {pct(values['redis'][0], values['crabpack'][0])} | {ratio:.2f}× |")

text.append('''
Leituras aleatórias de `session:*`, 100% hits, uma thread de I/O, 48 conexões, 10 s/rodada.
Após cada rodada, todos os valores são conferidos; no CrabPack, a contagem de comprimidos deve
continuar em 300.000. Cada célula mostra throughput [mín–máx] e mediana de p50/p99 em ms.

| Pipeline | Redis ops/s · p50/p99 ms | CrabCache ops/s · p50/p99 ms | CrabPack ops/s · p50/p99 ms |
|---|---|---|---|''')
for pipeline in [1, 16]:
    cells = []
    for server in ['redis', 'crabcache', 'crabpack']:
        rows = selected('compression', mode='compression', server=server, pipeline=pipeline)
        cells.append(f"{spread(metric(rows, 'ops_s'))} · {metric(rows, 'p50_ms')[0]:.3f}/{metric(rows, 'p99_ms')[0]:.3f}")
    text.append(f"| {pipeline} | {' | '.join(cells)} |")

text.append('''
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
''')
output.write_text('\n'.join(text).rstrip() + '\n')
print(output)
