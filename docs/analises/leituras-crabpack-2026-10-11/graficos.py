#!/usr/bin/env python3
"""Recalculate and plot this analysis from raw, independently checked measurements.

Usage: python graficos.py <raw-directory> [output-directory] [memory-timeline-json]
Requires matplotlib. Keeps old, published benchmarks separate from this exercise.
"""
import csv
import contextlib
import hashlib
import json
import io
import math
from pathlib import Path
import statistics
import sys
import zipfile

import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt
import numpy as np

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / 'scripts'))
from benchmark import read_resp, encode

RAW = Path(sys.argv[1]).resolve()
OUT = Path(sys.argv[2]).resolve() if len(sys.argv) > 2 else Path(__file__).resolve().parent
OUT.mkdir(parents=True, exist_ok=True)
data = json.loads((RAW / 'summary.json').read_text())
meta, rows = data['metadata'], data['results']
datasets = meta['datasets']
repeats, n = meta['repeats'], meta['keys_per_dataset']


@contextlib.contextmanager
def open_dataset(name):
    path = RAW / name
    if path.exists():
        with path.open('rb') as stream:
            yield stream
    else:
        with zipfile.ZipFile(RAW / 'traces.zip') as archive:
            with archive.open(name) as entry, io.BufferedReader(entry) as stream:
                yield stream


def close(actual, expected, label):
    if not math.isclose(actual, expected, rel_tol=1e-9, abs_tol=1e-9):
        raise ValueError(f'{label}: {actual} != {expected}')


def selected(mode, name, server, config=None):
    matches = [r for r in rows if r['mode'] == mode and r['dataset'] == name
               and r['server'] == server and (config is None or r.get('config') == config)]
    assert len(matches) == repeats, (mode, name, server, config, len(matches))
    assert sorted(r['repeat'] for r in matches) == list(range(1, repeats + 1))
    return matches


def audit():
    assert len(rows) == len(datasets) * repeats * 2 * 4
    for d in datasets:
        path = RAW / f'{d["dataset"]}.resp'
        digest = hashlib.sha256()
        count = total = 0
        keys = set()
        with open_dataset(path.name) as stream:
            while stream.peek(1):
                command = read_resp(stream)
                assert len(command) == 3 and command[0] == b'SET'
                assert command[1].startswith(d['prefix'].encode())
                digest.update(encode(command))
                keys.add(command[1]); count += 1; total += len(command[2])
        assert count == n and len(keys) == n
        assert digest.hexdigest() == d['sha256']
        close(total / n, d['mean_value_bytes'], 'input bytes')
    for r in rows:
        dataset = next(d for d in datasets if d['dataset'] == r['dataset'])
        if r['mode'] == 'get-analysis':
            raw = json.loads((RAW / r['raw_json']).read_text())['ALL STATS']
            totals, gets = raw['Totals'], raw['Gets']
            assert totals['Count'] == gets['Count'] == r['client_get_count']
            assert gets['Misses/sec'] == 0 and totals['Connection Errors'] == 0
            assert raw['Runtime']['Interrupted'] == 'false'
            close(r['mean_latency_ms'], totals['Accumulated Latency'] / totals['Count'], 'mean ms')
            close(r['cpu_us_per_get'], r['cpu_seconds'] * 1e6 / totals['Count'], 'CPU us/GET')
            close(r['mean_cpu_cores'], r['cpu_seconds'] / r['wall_seconds'], 'CPU cores')
            close(r['ops_s'], totals['Ops/sec'], 'throughput')
            for field, rawfield in [('p50_ms', 'p50.00'), ('p99_ms', 'p99.00')]:
                close(r[field], totals['Percentile Latencies'][rawfield], field)
            assert 0 <= r['server_get_hits'] - r['client_get_count'] <= r['connections'] * r['pipeline']
            assert r['verified_values'] == n
        elif r['mode'] == 'memory-analysis':
            close(r['bytes_per_key'], (r['final_bytes'] - r['baseline_bytes']) / n, 'memory delta')
            assert r['verified_values'] == n
        elif r['mode'] == 'application-analysis':
            close(r['ready_object_mean_us'], r['get_mean_us'] + r['processing_mean_us'], 'application total')
            assert r['samples'] == 2500 and r['ready_object_p99_us'] >= r['ready_object_p50_us']
        if r['mode'] != 'application-analysis':
            expected = n if r['server'] == 'crabpack' else 0
            assert r['compressed_keys'] == expected
    # Check repetition coverage separately from record count.
    for d in datasets:
        for server in ('crabcache', 'crabpack'):
            selected('memory-analysis', d['dataset'], server)
            selected('application-analysis', d['dataset'], server)
            for config in ('single', 'pipeline16'):
                selected('get-analysis', d['dataset'], server, config)


def aggregate(d, server):
    result = {'dataset': d['dataset'], 'label': d['label'], 'server': server,
              'value_bytes': d['mean_value_bytes']}
    for config in ('single', 'pipeline16'):
        rs = selected('get-analysis', d['dataset'], server, config)
        count = sum(r['client_get_count'] for r in rs)
        result[f'{config}_mean_ms'] = sum(r['accumulated_latency_ms'] for r in rs) / count
        result[f'{config}_p50_ms'] = statistics.median(r['p50_ms'] for r in rs)
        result[f'{config}_p99_ms'] = statistics.median(r['p99_ms'] for r in rs)
        result[f'{config}_cpu_us'] = sum(r['cpu_seconds'] for r in rs) * 1e6 / count
        result[f'{config}_ops_s'] = statistics.median(r['ops_s'] for r in rs)
        result[f'{config}_mean_ms_range'] = [min(r['mean_latency_ms'] for r in rs), max(r['mean_latency_ms'] for r in rs)]
        result[f'{config}_p99_ms_range'] = [min(r['p99_ms'] for r in rs), max(r['p99_ms'] for r in rs)]
        result[f'{config}_cpu_us_range'] = [min(r['cpu_us_per_get'] for r in rs), max(r['cpu_us_per_get'] for r in rs)]
    memory = selected('memory-analysis', d['dataset'], server)
    result['physical_bytes_per_key'] = statistics.median(r['bytes_per_key'] for r in memory)
    result['physical_bytes_per_key_range'] = [min(r['bytes_per_key'] for r in memory), max(r['bytes_per_key'] for r in memory)]
    result['compressed_keys'] = statistics.median(r['compressed_keys'] for r in memory)
    app = selected('application-analysis', d['dataset'], server)
    for field in ('get_mean_us', 'processing_mean_us', 'ready_object_mean_us'):
        result['app_' + field] = statistics.mean(r[field] for r in app)
        result['app_' + field + '_range'] = [min(r[field] for r in app), max(r[field] for r in app)]
    return result


audit()
aggregates = [aggregate(d, server) for d in datasets for server in ('crabcache', 'crabpack')]
(OUT / 'dados-conferidos.json').write_text(json.dumps({'metadata': meta, 'results': aggregates}, indent=2) + '\n')
scalar_fields = [k for k, v in aggregates[0].items() if not isinstance(v, list)]
with (OUT / 'dados-conferidos.csv').open('w', newline='') as output:
    writer = csv.DictWriter(output, fieldnames=scalar_fields, extrasaction='ignore')
    writer.writeheader(); writer.writerows(aggregates)

plt.rcParams.update({'font.family': 'DejaVu Sans', 'font.size': 12,
                     'axes.spines.top': False, 'axes.spines.right': False,
                     'axes.spines.left': False, 'axes.edgecolor': '#cbd5e1',
                     'axes.titleweight': 'bold', 'axes.labelcolor': '#334155',
                     'text.color': '#142b43', 'xtick.color': '#475569',
                     'ytick.color': '#334155', 'figure.facecolor': '#f7f9fc',
                     'axes.facecolor': '#f7f9fc', 'savefig.facecolor': '#f7f9fc'})
colors = {'crabcache': '#2365b5', 'crabpack': '#d96826'}
labels = [f'{d["label"]}\n{d["mean_value_bytes"]:.0f} B por valor' for d in datasets]


def plot_pairs(ax, metric, title, unit, scale=1, digits=2):
    positions = np.arange(len(datasets))
    maximum = 0
    for server, offset, legend in [('crabcache', -.17, 'CrabCache · CrabPack desligado'),
                                   ('crabpack', .17, 'CrabCache · CrabPack ligado')]:
        rs = [next(a for a in aggregates if a['dataset'] == d['dataset'] and a['server'] == server)
              for d in datasets]
        values = np.array([r[metric] * scale for r in rs])
        ranges = np.array([r[metric + '_range'] for r in rs]) * scale
        errors = np.array([values - ranges[:, 0], ranges[:, 1] - values])
        ax.barh(positions + offset, values, height=.29, color=colors[server],
                xerr=np.maximum(errors, 0), error_kw={'elinewidth': 1, 'capsize': 2, 'alpha': .7},
                label=legend, zorder=3)
        maximum = max(maximum, max(ranges[:, 1]))
        for y, value, bound in zip(positions + offset, values, ranges[:, 1]):
            ax.annotate(f'{value:.{digits}f}', (bound, y), xytext=(5, 0),
                        textcoords='offset points', va='center', color=colors[server], fontsize=10)
    ax.set_yticks(positions, labels, fontsize=10.5)
    ax.invert_yaxis()
    ax.set_xlim(0, maximum * 1.25)
    ax.set_title(title, loc='left', fontsize=15, pad=14)
    ax.set_xlabel(unit + ' · menor é melhor', fontsize=11)
    ax.grid(axis='x', color='#dce4ed', zorder=0)
    ax.tick_params(axis='y', length=0, pad=8)


fig, axes = plt.subplots(3, 2, figsize=(19, 17))
fig.subplots_adjust(left=.15, right=.97, bottom=.13, top=.86, hspace=.47, wspace=.59)
fig.text(.06, .97, 'Quanto custa ler um valor do CrabCache?', fontsize=29, weight='bold')
fig.text(.06, .941, 'CrabPack: representação menor, com custo de CPU e latência dependente do objeto e da concorrência.', fontsize=15)
fig.text(.06, .915, f'{meta["cpu_model"]} · execução nativa / loopback · 1 thread de I/O · {n:,} chaves por conjunto · {repeats} × {meta["seconds"]} s por cenário'.replace(',', '.'), fontsize=12, color='#475569')
handles = [plt.Line2D([0], [0], color=colors[s], lw=10) for s in ('crabcache', 'crabpack')]
fig.legend(handles, ['CrabPack desligado', 'CrabPack ligado'], loc='upper left',
           bbox_to_anchor=(.06, .905), ncol=2, frameon=False, fontsize=13)
plot_pairs(axes[0, 0], 'single_mean_ms', '01  GET individual · latência média', 'Microssegundos (µs)', scale=1000)
plot_pairs(axes[0, 1], 'single_p99_ms', '02  GET individual · p99', 'Microssegundos (µs)', scale=1000, digits=0)
plot_pairs(axes[1, 0], 'pipeline16_mean_ms', '03  GET sob carga · 48 conexões / pipeline 16', 'Milissegundos (ms)', digits=3)
plot_pairs(axes[1, 1], 'pipeline16_cpu_us', '04  CPU do servidor por GET · mesma carga', 'Microssegundos de CPU / GET', digits=3)
plot_pairs(axes[2, 0], 'physical_bytes_per_key', '05  RAM por chave · amostra logo após treino', 'Bytes / chave', digits=0)
plot_pairs(axes[2, 1], 'app_ready_object_mean_us', '06  GET + objeto pronto · cliente Python', 'Microssegundos (µs)', digits=2)
fig.text(.06, .088, 'CrabPack: 100% das chaves comprimidas antes de medir. O GET devolve o JSON completo; comprimir o armazenamento não reduz o tráfego do GET.', fontsize=12, weight='bold')
fig.text(.06, .065, 'GET inclui rede local e resposta; CPU/GET mede o servidor. RAM foi amostrada 0,5 s após preparar o conjunto; não indica consumo estável nem pico.', fontsize=11, color='#475569')
fig.text(.06, .042, 'Barras: média ponderada de GET/CPU; mediana para memória e p99; média para objeto pronto. Traços: mínimo–máximo das 3 rodadas, sem intervalo de confiança.', fontsize=10.5, color='#475569')
fig.text(.06, .022, 'Análise complementar · 11/10/2026 · dados sintéticos, máquina compartilhada, cache pré-carregado e zero misses · não mede TLS, S3 nem restauração após falha.', fontsize=10.5, color='#475569')
for suffix in ('png', 'svg', 'pdf'):
    fig.savefig(OUT / f'comparacao-leituras.{suffix}', dpi=165)
plt.close(fig)

# A second figure exposes where object decoding takes place.
fig, axes = plt.subplots(1, 2, figsize=(17, 8))
fig.subplots_adjust(left=.16, right=.96, bottom=.19, top=.77, wspace=.43)
fig.text(.055, .95, 'Do GET ao objeto utilizável pela aplicação', fontsize=25, weight='bold')
fig.text(.055, .905, 'Uma conexão persistente · 2.500 leituras por rodada · inclui verificações de conteúdo · 3 rodadas', fontsize=12)
for ax, server, title in zip(axes, ('crabcache', 'crabpack'), ('CrabPack desligado', 'CrabPack ligado')):
    rs = [next(a for a in aggregates if a['dataset'] == d['dataset'] and a['server'] == server) for d in datasets]
    gets = np.array([r['app_get_mean_us'] for r in rs])
    processing = np.array([r['app_processing_mean_us'] for r in rs])
    ax.barh(np.arange(len(rs)), gets, color='#2365b5', height=.55, label='GET: rede + servidor')
    ax.barh(np.arange(len(rs)), processing, left=gets, color='#7e5fc0', height=.55,
            label='Cliente: conferir e interpretar JSON')
    ranges = np.array([r['app_ready_object_mean_us_range'] for r in rs])
    totals = gets + processing
    ax.errorbar(totals, np.arange(len(rs)),
                xerr=np.maximum(np.array([totals-ranges[:, 0], ranges[:, 1]-totals]), 0),
                fmt='none', ecolor='#334155', capsize=3, linewidth=1)
    for index, total in enumerate(gets + processing):
        ax.text(ranges[index, 1] + 1, index, f'{total:.2f} µs', va='center', fontsize=10)
    ax.set_yticks(np.arange(len(rs)), labels, fontsize=10)
    ax.invert_yaxis(); ax.set_xlim(0, max(gets + processing) * 1.25)
    ax.set_title(title, fontsize=16, loc='left'); ax.grid(axis='x', color='#dce4ed')
    ax.set_axisbelow(True); ax.set_xlabel('Tempo médio por objeto (µs)'); ax.tick_params(axis='y', length=0)
app_max = max(a['app_ready_object_mean_us_range'][1] for a in aggregates)
for ax in axes:
    ax.set_xlim(0, app_max * 1.2)
handles, names = axes[0].get_legend_handles_labels()
fig.legend(handles, names, loc='upper left', bbox_to_anchor=(.055, .865), ncol=2, frameon=False)
fig.text(.055, .105, 'Com CrabPack, a descompressão acontece no servidor e está incluída no GET. A aplicação recebe os bytes originais e interpreta o JSON.', fontsize=11)
fig.text(.055, .07, 'Barras: média de 3 rodadas; traços: mínimo–máximo. Variação ampla não demonstra que o Pack acelere o processamento da aplicação.', fontsize=11, color='#475569')
for suffix in ('png', 'svg', 'pdf'):
    fig.savefig(OUT / f'objeto-pronto.{suffix}', dpi=165)
plt.close(fig)

# A paired diagnostic follows RAM after training. It is not a repeated benchmark
# and is kept explicit so the short memory sample is not sold as steady-state RAM.
timeline_path = (Path(sys.argv[3]).resolve() if len(sys.argv) > 3 else
                 RAW / 'memory-timeline.json' if (RAW / 'memory-timeline.json').exists()
                 else OUT / 'memoria-20s.json')
timeline = json.loads(timeline_path.read_text())
timeline_rows = timeline['results']
assert len(timeline_rows) == 10
for server in ('crabcache', 'crabpack'):
    records = [r for r in timeline_rows if r['server'] == server]
    assert [r['sample_after_seconds'] for r in records] == [.5, 2, 5, 12, 20]
    for r in records:
        close(r['incremental_bytes_per_key'],
              (r['physical_bytes'] - r['baseline_bytes']) / n, 'timeline RAM')
        assert r['compressed_keys'] == (n if server == 'crabpack' else 0)
(OUT / 'memoria-20s.json').write_text(json.dumps(timeline, indent=2) + '\n')
fig, axes = plt.subplots(1, 2, figsize=(15, 6.5))
fig.subplots_adjust(left=.08, right=.97, bottom=.24, top=.72, wspace=.25)
fig.text(.055, .94, 'Memória depois do treino: armazenamento e RAM', fontsize=22, weight='bold')
fig.text(.055, .895, 'Diagnóstico adicional · 10.000 sessões JSON · uma rodada pareada · acompanhamento até 20 segundos', fontsize=11)
for server, label in [('crabcache', 'CrabPack desligado'), ('crabpack', 'CrabPack ligado')]:
    rs = [r for r in timeline_rows if r['server'] == server]
    x = [r['elapsed_seconds'] for r in rs]
    axes[0].plot(x, [(r['physical_bytes']-r['baseline_bytes'])/1024**2 for r in rs],
                 color=colors[server], marker='o', label=label)
    axes[1].plot(x, [r['used_memory_bytes_per_key']*n/1024**2 for r in rs],
                 color=colors[server], marker='o', label=label)
for ax, title in zip(axes, ['RAM física incremental do processo', 'Dados contabilizados internamente']):
    ax.set_title(title, loc='left', fontsize=13)
    ax.set_ylim(bottom=0); ax.set_xlabel('Segundos após concluir a compactação')
    ax.set_ylabel('MiB'); ax.grid(color='#dce4ed'); ax.set_axisbelow(True)
fig.legend(*axes[0].get_legend_handles_labels(), loc='upper left',
           bbox_to_anchor=(.055, .835), ncol=2, frameon=False)
fig.text(.055, .13, 'Os dados armazenados ficaram menores, mas a RAM física permaneceu maior com Pack nesta base pequena durante os 20 s observados.', fontsize=10.5)
fig.text(.055, .075, 'A medição não isola dicionários, tabelas zstd, memória retida do treino e allocator; não estabelece o consumo em prazos maiores.', fontsize=10.5, color='#475569')
for suffix in ('png', 'svg', 'pdf'):
    fig.savefig(OUT / f'memoria-apos-treino.{suffix}', dpi=165)
plt.close(fig)

# Recalculate the earlier audited session workload independently, without mixing
# its 300k-key working set into the new 10k-key experiment.
oldroot = ROOT / 'docs/benchmark-results/2026-10-09/compression'
old = json.loads((oldroot / 'summary.json').read_text())
historical = []
for pipeline in (1, 16):
    for server in ('crabcache', 'crabpack'):
        matches = [r for r in old['results'] if r['mode'] == 'compression'
                   and r['pipeline'] == pipeline and r['server'] == server]
        raw = [json.loads((oldroot / f'{old["results"].index(r):03d}-{server}-p{pipeline}.memtier.json').read_text())['ALL STATS']['Totals'] for r in matches]
        historical.append({'pipeline': pipeline, 'server': server,
                           'mean_ms': sum(r['Accumulated Latency'] for r in raw) / sum(r['Count'] for r in raw),
                           'p99_ms': statistics.median(r['p99_ms'] for r in matches),
                           'cpu_us': sum(r['cpu_seconds'] for r in matches) * 1e6 / sum(r['ops_count'] for r in matches)})
(OUT / 'auditoria-anterior.json').write_text(json.dumps(historical, indent=2) + '\n')


def pt(value, digits=3):
    return f'{value:.{digits}f}'.replace('.', ',')


lines = [
    '# Leituras do CrabCache: CPU, CrabPack e objetos comprimidos',
    '',
    'Análise complementar de **11/10/2026**, mantendo separados os benchmarks auditados de 09/10.',
    'O custo de ler existe com e sem compressão. Com CrabPack, o GET também descomprime o valor.',
    'A aplicação recebe os bytes originais e interpreta o JSON. Compressão não é criptografia.',
    '',
    '![Comparação completa de leituras](comparacao-leituras.png)',
    '',
    '## Tempos e CPU por objeto',
    '',
    'Média de GET em uma conexão, média sob carga de 48 conexões/pipeline 16 e CPU do servidor nessa carga.',
    'As médias de GET e CPU são ponderadas pelo número de respostas de cada rodada.',
    '',
    '| Objeto | Valor médio | GET individual sem / com Pack (µs) | GET sob carga sem / com Pack (ms) | CPU sob carga sem / com Pack (µs CPU/GET) | Chaves comprimidas com Pack |',
    '|---|---|---|---|---|---|',
]
for d in datasets:
    plain, packed = [next(a for a in aggregates if a['dataset'] == d['dataset'] and a['server'] == s) for s in ('crabcache', 'crabpack')]
    lines.append(f'| {d["label"]} | {pt(d["mean_value_bytes"], 0)} B | '
                 f'{pt(plain["single_mean_ms"]*1000, 2)} / {pt(packed["single_mean_ms"]*1000, 2)} | '
                 f'{pt(plain["pipeline16_mean_ms"])} / {pt(packed["pipeline16_mean_ms"])} | '
                 f'{pt(plain["pipeline16_cpu_us"])} / {pt(packed["pipeline16_cpu_us"])} | '
                 f'{packed["compressed_keys"] / n:.0%} |')
lines += ['', '**µs = microssegundo; 1.000 µs = 1 ms.** CPU/GET é trabalho do processo servidor,',
          'não o tempo de espera do cliente. Não use `1 / ops/s` como se fosse latência média.',
          '', '## Até o objeto ficar pronto na aplicação', '',
          '![GET e interpretação do JSON](objeto-pronto.png)', '',
          'O tempo total é medido na mesma requisição: GET + verificações de conteúdo + conversão JSON',
          'no cliente Python. Não soma medições de cargas diferentes.',
          '', '| Objeto | Sem Pack: GET / processamento / total (µs) | Com Pack: GET / processamento / total (µs) |',
          '|---|---|---|']
for d in datasets:
    pair = [next(a for a in aggregates if a['dataset'] == d['dataset'] and a['server'] == s) for s in ('crabcache', 'crabpack')]
    text = [' / '.join(pt(a[field], 2) for field in ('app_get_mean_us', 'app_processing_mean_us', 'app_ready_object_mean_us')) for a in pair]
    lines.append(f'| {d["label"]} | {text[0]} | {text[1]} |')
lines += ['', '## Exemplos reais dos objetos sintéticos deste teste', '']
for d in datasets:
    example = dict(d['example_plaintext'])
    if d['dataset'] == 'api_large':
        example['data'] = dict(example['data'])
        example['data']['items'] = example['data']['items'][:2]
        example['nota_do_relatorio'] = 'Mostrados 2 dos 40 itens; o teste usa todos.'
    lines += [f'### {d["label"]} · {pt(d["mean_value_bytes"], 0)} B em média', '',
              '```json', json.dumps(example, ensure_ascii=False, indent=2), '```', '']
lines += [
    '## Onde a eficiência aparece', '',
    'O benefício direto do CrabPack é reduzir a memória do armazenamento. Descomprimir usa CPU',
    'e pode aumentar a latência ou limitar o throughput. O GET continua transmitindo o valor original:',
    'a compressão interna não é uma compressão do protocolo de rede.',
    'Valores maiores também exigem mais cópia/transmissão e mais trabalho para interpretar JSON.',
    '', '## Memória: representação menor não garante menos RAM numa base pequena', '',
    'No ensaio de 10 mil chaves, o footprint logo após o treino foi maior com Pack para os três',
    'conjuntos pequenos. Na página de API com 40 itens, a amostra ficou menor com Pack.',
    'Os valores comprimidos ocupam menos bytes internos, mas a RAM física inclui outros custos.',
    '', '![Memória após o treino](memoria-apos-treino.png)', '',
    'Uma verificação adicional pareada de sessões acompanhou os dois servidores até 20 segundos',
    'após concluir a compactação. A contabilização interna caiu com Pack, mas o footprint permaneceu',
    'maior durante todo o período observado. Não foi apenas uma diferença que desapareceu após 2 segundos.',
    'Esta verificação tem uma rodada, não três, e não isola a contribuição de dicionários, tabelas zstd,',
    'memória retida do treino e comportamento do allocator. Não permite concluir quanto tempo esses custos persistem.',
    '',
    'A auditoria anterior tinha **300 mil chaves**, e mostrou redução de footprint com CrabPack nesses',
    'JSONs maiores em volume. Portanto, a quantidade de chaves e os custos fixos importam:',
    'não extrapole a economia da base grande para toda base pequena. Compare RAM física, não só `used_memory`.',
    '', '## Método e limites', '',
    f'- {meta["cpu_model"]}; {meta["platform"]}; {meta["cpu_count"]} CPUs lógicas; {meta["memory_bytes"] / 1024**3:.0f} GiB.',
    f'- CrabCache 0.3.0 nativo, build release; hash SHA-256 do binário: `{meta["binary_sha256"]}`.',
    f'- Python `{meta["python"].split()[0]}`; memtier conforme metadados.',
    f'- {n:,} chaves por conjunto, {repeats} rodadas de {meta["seconds"]} segundos por condição; ordem dos servidores e das cargas invertida na segunda rodada.',
    '- Uma instância nova por conjunto/servidor/rodada; servidor com uma thread de I/O, sem afinidade a núcleo.',
    '- Cliente e servidor compartilham o Mac, por loopback; outros aplicativos continuam ativos.',
    '- Leituras GET aleatórias: 1 cliente sem pipeline; ou 4 threads × 12 conexões, pipeline 16.',
    '- Cache pré-carregado. Zero erros de carga e exatamente N respostas; DBSIZE=N; zero misses.',
    '- JSON: todas as chaves comprimidas antes de medir Pack.',
    '- Conferência de todas as chaves após cada carga de GET, fora do intervalo medido.',
    '- Memória física: footprint do macOS final menos baseline, dividido por N; mediana de três amostras por rodada.',
    '- Memória coletada antes das leituras de validação completa; espera de 0,5 s após preparar a compressão. Não mede pico de treino.',
    '- CPU: diferença do tempo cumulativo de todo o processo, via `ps time`, dividida pelos GETs contabilizados pelo cliente.',
    '- CPU inclui threads auxiliares e o trabalho em segundo plano do CrabPack. A resolução de `ps time` limita a precisão.',
    '- Medição de objeto pronto: 2.500 GETs por rodada, uma conexão Python persistente, acesso aleatório e aquecimento de 100 requisições.',
    '- Média: acumulado de latências / contagem total. p99: mediana dos p99 das rodadas, não percentil global agrupado.',
    '- Traços no gráfico indicam mínimo–máximo observado, não intervalos de confiança.',
    '- Este ensaio é curto e usa conjuntos menores que a auditoria anterior. Não misture números como se fossem a mesma carga.',
    '- Não mede S3, disco, restauração após reiniciar, recuperação de backup, TLS nem o tempo isolado de uma função zstd.',
    '- Estes dados sintéticos não garantem latência ou economia para objetos de produção.',
    '', '## O que os benchmarks anteriores já mostravam', '',
    'Sessões JSON de aproximadamente 297 B, 300.000 chaves, 48 conexões e três rodadas de 10 segundos.',
    'Médias recalculadas a partir de `Accumulated Latency / Count` nos JSONs publicados.',
    '', '| Pipeline | GET médio sem / com Pack (ms) | p99 sem / com Pack (ms) | CPU sem / com Pack (µs CPU/GET) |',
    '|---|---|---|---|',
]
for pipeline in (1, 16):
    pair = [next(r for r in historical if r['pipeline'] == pipeline and r['server'] == s) for s in ('crabcache', 'crabpack')]
    lines.append(f'| {pipeline} | {pt(pair[0]["mean_ms"])} / {pt(pair[1]["mean_ms"])} | '
                 f'{pt(pair[0]["p99_ms"])} / {pt(pair[1]["p99_ms"])} | {pt(pair[0]["cpu_us"])} / {pt(pair[1]["cpu_us"])} |')
lines += ['', '[Auditoria publicada](../../BENCHMARKS.md).', '',
          '## Reproduzir e conferir', '',
          'O código da aplicação não foi alterado para este ensaio. Os scripts desta pasta geram',
          'novos artefatos locais, preservando os benchmarks publicados.', '',
          '```bash', 'python3 -m venv --system-site-packages target/analysis-venv',
          'target/analysis-venv/bin/python -m pip install matplotlib',
          'SECS=4 REPEATS=3 KEYS=10000 target/analysis-venv/bin/python docs/analises/leituras-crabpack-2026-10-11/medir.py',
          'KEYS=10000 target/analysis-venv/bin/python docs/analises/leituras-crabpack-2026-10-11/memoria.py',
          '# Cada script anuncia seu diretório novo. Use os caminhos entre aspas:',
          'target/analysis-venv/bin/python docs/analises/leituras-crabpack-2026-10-11/graficos.py "CAMINHO-DA-MEDICAO" "PASTA-DOS-GRAFICOS" "CAMINHO-DA-MEMORIA/summary.json"',
          '```', '',
          '`graficos.py` verifica contagens, erros, misses, cobertura de compressão, hashes dos conjuntos,',
          'latências a partir dos JSONs brutos, fórmulas de CPU/memória e decomposição do tempo do cliente.',
          '', f'Artefatos brutos desta execução: `{RAW.relative_to(ROOT)}`.',
          '', '[Dados agregados CSV](dados-conferidos.csv) · [Dados e metadados JSON](dados-conferidos.json) ·',
          '[Gráfico vetorial](comparacao-leituras.svg) · [PDF](comparacao-leituras.pdf)', '']
(OUT / 'RELATORIO.md').write_text('\n'.join(lines))
print(f'Independent audit passed: {len(rows)} records; inputs, GETs, CPU, memory, app totals and repetitions checked.')
print('Graphs and report:', OUT)
