#!/usr/bin/env python3
"""Check recorded GET evidence and generate the README summary without servers.

python3 scripts/report-read-analysis.py          # update marked README blocks/chart
python3 scripts/report-read-analysis.py --check  # CI: reject stale or edited numbers

Standard library only. This checks recorded measurements; it never fabricates
fresh measurements or runs a performance benchmark on a CI runner.
"""
import argparse
import contextlib
import csv
from html import escape
import io
import json
import math
from pathlib import Path
import re
import statistics
import sys
import zipfile

from benchmark import encode, read_resp, require

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_DATA = ROOT / 'docs/benchmark-results/2026-10-11-read-analysis'
DEFAULT_ANALYSIS = ROOT / 'docs/analises/leituras-crabpack-2026-10-11'


def close(actual, expected, label):
    require(math.isfinite(actual) and math.isfinite(expected)
            and math.isclose(actual, expected, rel_tol=1e-9, abs_tol=1e-9), label)


class Analysis:
    def __init__(self, folder):
        self.folder = folder
        self.data = json.loads((folder / 'summary.json').read_text())
        self.meta = self.data['metadata']
        self.rows = self.data['results']
        self.datasets = self.meta['datasets']
        self.n = self.meta['keys_per_dataset']
        self.repeats = self.meta['repeats']

    @contextlib.contextmanager
    def input(self, name):
        path = self.folder / name
        if path.exists():
            with path.open('rb') as stream:
                yield stream
        else:
            with zipfile.ZipFile(self.folder / 'traces.zip') as archive:
                with archive.open(name) as entry, io.BufferedReader(entry) as stream:
                    yield stream

    def selected(self, mode, dataset, server, config=None):
        rows = [r for r in self.rows if r['mode'] == mode and r['dataset'] == dataset
                and r['server'] == server and (config is None or r.get('config') == config)]
        require(len(rows) == self.repeats and sorted(r['repeat'] for r in rows)
                == list(range(1, self.repeats + 1)), 'missing or duplicate repetition')
        return rows

    def audit(self):
        import hashlib
        require(self.repeats == 3 and self.n > 1, 'unexpected repetition/key settings')
        require(len(self.rows) == len(self.datasets) * self.repeats * 2 * 4, 'incomplete analysis')
        require({d['dataset'] for d in self.datasets} == {'session', 'product', 'api', 'api_large'},
                'unexpected object datasets')
        for d in self.datasets:
            digest = hashlib.sha256()
            count = total = 0
            keys = set()
            with self.input(d['dataset'] + '.resp') as stream:
                while stream.peek(1):
                    command = read_resp(stream)
                    require(len(command) == 3 and command[0] == b'SET', 'invalid input command')
                    require(command[1].startswith(d['prefix'].encode()), 'wrong key prefix')
                    digest.update(encode(command))
                    keys.add(command[1]); count += 1; total += len(command[2])
            require(count == self.n and len(keys) == self.n, 'input key count mismatch')
            require(digest.hexdigest() == d['sha256'], 'input hash mismatch')
            close(total / self.n, d['mean_value_bytes'], 'input byte count mismatch')
        loads = {}
        for r in self.rows:
            context = (r['dataset'], r['server'], r['repeat'])
            require(context not in loads or loads[context] == r['load_stdout'], 'load context mismatch')
            loads[context] = r['load_stdout']
            if r['mode'] == 'get-analysis':
                raw = json.loads((self.folder / r['raw_json']).read_text())['ALL STATS']
                totals, gets = raw['Totals'], raw['Gets']
                require(totals['Count'] > 0 and r['cpu_seconds'] > 0 and r['wall_seconds'] > 0,
                        'nonpositive count or CPU/wall timer')
                require(totals['Count'] == gets['Count'] == r['client_get_count'], 'GET count mismatch')
                require(gets['Misses/sec'] == 0 and all(raw[section]['Connection Errors'] == 0
                        for section in ('Sets', 'Gets', 'Totals')), 'GET errors/misses')
                require(raw['Runtime']['Interrupted'] == 'false', 'interrupted GET run')
                require(raw['Runtime']['Time unit'] == 'MILLISECONDS'
                        and abs(raw['Runtime']['Total duration']/1000-self.meta['seconds']) < .02,
                        'recorded GET duration disagrees with metadata')
                close(r['mean_latency_ms'], totals['Accumulated Latency'] / totals['Count'], 'latency mismatch')
                close(r['accumulated_latency_ms'], totals['Accumulated Latency'], 'accumulated latency mismatch')
                close(r['cpu_us_per_get'], r['cpu_seconds'] * 1e6 / totals['Count'], 'CPU/GET mismatch')
                close(r['mean_cpu_cores'], r['cpu_seconds'] / r['wall_seconds'], 'CPU cores mismatch')
                close(r['ops_s'], totals['Ops/sec'], 'throughput mismatch')
                for field, rawfield in [('p50_ms', 'p50.00'), ('p99_ms', 'p99.00')]:
                    close(r[field], totals['Percentile Latencies'][rawfield], field + ' mismatch')
                require(0 <= r['server_get_hits'] - r['client_get_count']
                        <= r['connections'] * r['pipeline'], 'unreported GETs exceed pipeline bound')
                expected_config = (1, 1) if r['config'] == 'single' else (48, 16)
                require((r['connections'], r['pipeline']) == expected_config, 'wrong GET configuration')
                require(r['verified_values'] == self.n, 'incomplete readback')
            elif r['mode'] == 'memory-analysis':
                close(r['bytes_per_key'], (r['final_bytes'] - r['baseline_bytes']) / self.n, 'memory mismatch')
                require(r['verified_values'] == self.n, 'incomplete memory readback')
            elif r['mode'] == 'application-analysis':
                close(r['ready_object_mean_us'], r['get_mean_us'] + r['processing_mean_us'], 'app timing mismatch')
                require(r['samples'] == 2500 and r['ready_object_p99_us'] >= r['ready_object_p50_us'], 'invalid app sample')
            else:
                require(False, 'unknown measurement mode')
            if r['mode'] != 'application-analysis':
                require(r['compressed_keys'] == (self.n if r['server'] == 'crabpack' else 0), 'packing coverage mismatch')
        require(len(loads) == len(self.datasets) * self.repeats * 2
                and len(set(loads.values())) == len(loads), 'missing/duplicate raw loads')
        for context, name in loads.items():
            with self.input(name) as stream:
                match = re.search(r'errors:\s*(\d+), replies:\s*(\d+)', stream.read().decode())
                require(match and int(match[1]) == 0 and int(match[2]) == self.n, 'invalid load output')
            serial = int(name.split('-')[0])-1
            with self.input(f'{serial:03d}-{context[1]}-server.command.json') as stream:
                command = json.loads(stream.read())
            require('--threads' in command and command[command.index('--threads')+1] == '1',
                    'recorded server command disagrees with one-thread configuration')
            require(('--compression' in command) == (context[1] == 'crabpack'),
                    'recorded compression flag mismatch')
        for d in self.datasets:
            for server in ('crabcache', 'crabpack'):
                for mode in ('memory-analysis', 'application-analysis'):
                    self.selected(mode, d['dataset'], server)
                for config in ('single', 'pipeline16'):
                    self.selected('get-analysis', d['dataset'], server, config)
        timeline = json.loads((self.folder / 'memory-timeline.json').read_text())['results']
        require(len(timeline) == 10, 'missing timeline samples')
        for server in ('crabcache', 'crabpack'):
            rows = [r for r in timeline if r['server'] == server]
            require([r['sample_after_seconds'] for r in rows] == [.5, 2, 5, 12, 20], 'timeline timestamps mismatch')
            for r in rows:
                close(r['incremental_bytes_per_key'], (r['physical_bytes']-r['baseline_bytes'])/self.n, 'timeline RAM mismatch')
                require(r['compressed_keys'] == (self.n if server == 'crabpack' else 0), 'timeline packing mismatch')

    def aggregate(self, d, server):
        result = {'dataset': d['dataset'], 'label': d['label'], 'server': server,
                  'value_bytes': d['mean_value_bytes']}
        for config in ('single', 'pipeline16'):
            rows = self.selected('get-analysis', d['dataset'], server, config)
            count = sum(r['client_get_count'] for r in rows)
            result[f'{config}_mean_ms'] = sum(r['accumulated_latency_ms'] for r in rows) / count
            for field in ('p50_ms', 'p99_ms', 'ops_s'):
                result[f'{config}_{field}'] = statistics.median(r[field] for r in rows)
            result[f'{config}_cpu_us'] = sum(r['cpu_seconds'] for r in rows) * 1e6 / count
            for source, target in [('mean_latency_ms', 'mean_ms'), ('p99_ms', 'p99_ms'), ('cpu_us_per_get', 'cpu_us')]:
                result[f'{config}_{target}_range'] = [min(r[source] for r in rows), max(r[source] for r in rows)]
        rows = self.selected('memory-analysis', d['dataset'], server)
        result['physical_bytes_per_key'] = statistics.median(r['bytes_per_key'] for r in rows)
        result['physical_bytes_per_key_range'] = [min(r['bytes_per_key'] for r in rows), max(r['bytes_per_key'] for r in rows)]
        result['compressed_keys'] = statistics.median(r['compressed_keys'] for r in rows)
        rows = self.selected('application-analysis', d['dataset'], server)
        for field in ('get_mean_us', 'processing_mean_us', 'ready_object_mean_us'):
            result['app_' + field] = statistics.mean(r[field] for r in rows)
            result['app_' + field + '_range'] = [min(r[field] for r in rows), max(r[field] for r in rows)]
        return result


def pt(value, digits=2):
    return f'{value:.{digits}f}'.replace('.', ',')


def pair(results, dataset):
    return [next(r for r in results if r['dataset'] == dataset and r['server'] == server)
            for server in ('crabcache', 'crabpack')]


def chart(analysis, results):
    # Repo-native SVG: deterministic regeneration without plotting-library/font versions in CI.
    parts = ['<svg xmlns="http://www.w3.org/2000/svg" width="1024" height="456" viewBox="0 0 1024 456" role="img" aria-labelledby="title desc">',
             '<title id="title">CrabPack: latência individual e CPU por leitura</title>',
             '<desc id="desc">Quatro conjuntos JSON, CrabPack desligado e ligado. Médias de três rodadas. Latência em uma conexão; CPU com 48 conexões e pipeline 16.</desc>',
             '<style>text{font-family:system-ui,sans-serif;fill:#142b43} .axis{stroke:#dce4ed} .plain{fill:#2365b5} .pack{fill:#d96826}</style>',
             '<rect width="1024" height="456" rx="10" fill="#f7f9fc"/>',
             '<text x="24" y="32" font-size="21" font-weight="600">CrabPack: tempo de leitura e custo de CPU</text>',
             f'<text x="24" y="55" font-size="13">{analysis.meta["measurement_date"]} · {analysis.n:,} chaves/conjunto · {analysis.repeats} × {analysis.meta["seconds"]} s · {escape(analysis.meta.get("cpu_model", analysis.meta["machine"]))} / loopback / uma thread de I/O</text>'.replace(',', '.'),
             '<rect x="24" y="69" width="13" height="13" class="plain"/><text x="44" y="81" font-size="13">CrabPack desligado</text>',
             '<rect x="230" y="69" width="13" height="13" class="pack"/><text x="250" y="81" font-size="13">CrabPack ligado · 100% comprimido</text>']
    for left, metric, multiplier, label in [(0, 'single_mean_ms', 1000, 'GET individual · média em µs'),
                                             (512, 'pipeline16_cpu_us', 1, 'CPU por GET · 48 conexões / pipeline 16 · µs')]:
        values = [r[metric] * multiplier for r in results]
        scale = 240 / max(values)
        x = left + 160
        parts.append(f'<text x="{left+24}" y="116" font-size="15" font-weight="600">{label}</text>')
        parts.append(f'<line x1="{x}" x2="{x}" y1="134" y2="387" class="axis"/>')
        for index, d in enumerate(analysis.datasets):
            y = 142 + index * 64
            name = {'session': 'Sessão', 'product': 'Produto', 'api': 'API', 'api_large': 'API · 40 itens'}[d['dataset']]
            parts.append(f'<text x="{x-10}" y="{y+13}" text-anchor="end" font-size="13">{escape(name)}</text>')
            parts.append(f'<text x="{x-10}" y="{y+31}" text-anchor="end" font-size="11">{d["mean_value_bytes"]:.0f} B por valor</text>')
            for offset, (r, cls) in enumerate(zip(pair(results, d['dataset']), ('plain', 'pack'))):
                value = r[metric] * multiplier
                width = value * scale
                yy = y + offset * 21
                parts.append(f'<rect x="{x}" y="{yy}" width="{width:.3f}" height="17" rx="2" class="{cls}"/>')
                parts.append(f'<text x="{x+width+6:.3f}" y="{yy+13}" font-size="12">{pt(value, 2 if multiplier==1000 else 3)}</text>')
    parts += ['<text x="24" y="418" font-size="12">Latência inclui rede e resposta; CPU mede o processo servidor. Menor é melhor. Não é tempo isolado de zstd.</text>',
              '<text x="24" y="440" font-size="12">Dados sintéticos e rodadas curtas. Variação, p99, RAM e metodologia estão no relatório completo.</text>', '</svg>']
    return '\n'.join(parts) + '\n'


def blocks(analysis, results):
    old = json.loads((ROOT / 'docs/benchmark-results/2026-10-09/compression/summary.json').read_text())
    savings = []
    for dataset in ('session', 'product', 'api'):
        def memory(server):
            return statistics.median(r['bytes_per_key'] for r in old['results']
                                     if r['mode']=='memory' and r['dataset']==dataset and r['server']==server)
        savings.append((1-memory('crabpack')/memory('redis'))*100)
    top = (f'**Novidade na v0.3.0 — CrabPack:** compressão opcional de valores semelhantes, como sessões e\n'
           f'respostas de API. Na auditoria de 09/10/2026, com **300 mil chaves por conjunto**, a economia mediana\n'
           f'de RAM incremental foi de **{min(savings):.0f}–{max(savings):.0f}% em relação ao Redis**, nos JSONs sintéticos medidos.\n'
           'Esse ganho depende do volume e dos dados: bases pequenas podem consumir mais RAM após o treino,\n'
           'e descomprimir custa CPU. Veja [CPU, latência e memória](#cpu-latência-e-memória-do-crabpack).\n'
           'A opção vem desligada; ative por flag, variável de ambiente ou `CONFIG SET`; veja [como usar](#ativando-o-crabpack).')
    date = analysis.meta['measurement_date'].split('-')
    key_count = f'{analysis.n:,}'.replace(',', '.')
    version = analysis.meta['crabcache'].removeprefix('crabcache ')
    lines = ['### CPU, latência e memória do CrabPack', '',
             f'Ensaio complementar de **{date[2]}/{date[1]}/{date[0]}**: **{key_count} chaves por conjunto**, três rodadas',
             f'de {analysis.meta["seconds"]} s, CrabCache {version} nativo no {analysis.meta.get("cpu_model", analysis.meta["machine"])}/{analysis.meta["platform"].split("-")[0]}, uma thread de I/O e loopback.',
             'Zero misses; os valores foram conferidos byte a byte. Estes dados não são a mesma carga da',
             'auditoria de 300 mil chaves acima. Médias ponderadas pelo número de GETs; **1.000 µs = 1 ms**.', '',
             '| Objeto JSON | Valor médio | GET individual sem / com CrabPack (µs) | CPU sob carga sem / com CrabPack (µs CPU/GET) |',
             '|---|---|---|---|']
    increases = []
    for d in analysis.datasets:
        plain, packed = pair(results, d['dataset'])
        increases.append((packed['pipeline16_cpu_us']/plain['pipeline16_cpu_us']-1)*100)
        lines.append(f'| {d["label"]} | {d["mean_value_bytes"]:.0f} B | {pt(plain["single_mean_ms"]*1000)} / {pt(packed["single_mean_ms"]*1000)} | {pt(plain["pipeline16_cpu_us"],3)} / {pt(packed["pipeline16_cpu_us"],3)} |')
    higher_ram = [d['label'] for d in analysis.datasets if pair(results, d['dataset'])[1]['physical_bytes_per_key']
                  > pair(results, d['dataset'])[0]['physical_bytes_per_key']]
    lower_ram = [d['label'] for d in analysis.datasets if pair(results, d['dataset'])[1]['physical_bytes_per_key']
                 < pair(results, d['dataset'])[0]['physical_bytes_per_key']]
    timeline = json.loads((analysis.folder/'memory-timeline.json').read_text())['results']
    by_time = {(r['server'], r['sample_after_seconds']): r['incremental_bytes_per_key'] for r in timeline}
    remained_higher = all(by_time['crabpack', second] > by_time['crabcache', second]
                         for second in (.5, 2, 5, 12, 20))
    ram_text = f'**Base de {key_count} chaves:** '
    if higher_ram:
        ram_text += 'a RAM observada após o treino ficou maior com CrabPack em ' + ', '.join(higher_ram) + '.'
    else:
        ram_text += 'o footprint pós-treino não ficou maior com CrabPack nos conjuntos medidos.'
    timeline_text = ('Uma verificação adicional de sessões acompanhou o efeito por 20 s, e a RAM continuou maior nesse intervalo.'
                     if remained_higher else
                     'Uma verificação adicional de sessões acompanhou a RAM por 20 s; os pontos estão no relatório completo.')
    if lower_ram:
        ram_text += '\nA amostra pós-treino ficou menor com CrabPack em ' + ', '.join(lower_ram) + '.'
    lines += ['', f'Com **48 conexões e pipeline 16**, a diferença de custo de CPU por GET foi de **{min(increases):+.0f}% a {max(increases):+.0f}%** com CrabPack.',
              'GET inclui comunicação local e resposta; CPU/GET mede o processo servidor, incluindo threads auxiliares.',
              'O cliente recebe o valor original: a compressão do armazenamento não reduz o tráfego do GET.', '',
              '<p align="center"><img src="docs/analises/leituras-crabpack-2026-10-11/resumo-readme.svg" alt="Médias de latência individual e CPU por GET, CrabPack desligado e ligado, para quatro conjuntos JSON." width="960"></p>', '',
              ram_text, timeline_text,
              'Comprimir os dados contabilizados não garante diminuir a RAM física do processo.',
              'Essas amostras não determinam o pico nem o consumo em prazos maiores; os custos não foram isolados.', '',
              'O [relatório completo](docs/analises/leituras-crabpack-2026-10-11/RELATORIO.md) mostra p99, faixas de variação,',
              'RAM, exemplos de objetos e tempo de converter JSON no cliente. As [medições e saídas brutas](docs/benchmark-results/2026-10-11-read-analysis)',
              'acompanham a documentação. O resumo e este gráfico são gerados desses dados, e o CI rejeita divergências.']
    return {'crabpack-memory-summary': top, 'crabpack-read-summary': '\n'.join(lines)}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--check', action='store_true')
    parser.add_argument('--data', type=Path, default=DEFAULT_DATA)
    parser.add_argument('--readme', type=Path, default=ROOT/'README.md')
    args = parser.parse_args()
    analysis = Analysis(args.data)
    analysis.audit()
    results = [analysis.aggregate(d, server) for d in analysis.datasets
               for server in ('crabcache', 'crabpack')]
    exported = json.loads((DEFAULT_ANALYSIS/'dados-conferidos.json').read_text())['results']
    require(len(exported) == len(results), 'analysis JSON disagrees with raw measurements')
    for row, result in zip(exported, results):
        require(set(row) == set(result), 'analysis JSON disagrees with raw measurements')
        for key, value in result.items():
            actual = row[key]
            if isinstance(value, list):
                require(isinstance(actual, list) and len(actual) == len(value),
                        'analysis JSON disagrees with raw measurements')
                for a, b in zip(actual, value):
                    close(a, b, 'analysis JSON disagrees with raw measurements')
            elif isinstance(value, (int, float)):
                close(actual, value, 'analysis JSON disagrees with raw measurements')
            else:
                require(actual == value, 'analysis JSON disagrees with raw measurements')
    with (DEFAULT_ANALYSIS/'dados-conferidos.csv').open(newline='') as stream:
        csv_rows = list(csv.DictReader(stream))
    require(len(csv_rows) == len(results), 'CSV count mismatch')
    for row, result in zip(csv_rows, results):
        for key, value in result.items():
            if isinstance(value, list):
                continue
            if isinstance(value, (int, float)):
                close(float(row[key]), value, 'CSV '+key+' mismatch')
            else:
                require(row[key] == value, 'CSV '+key+' mismatch')
    text = args.readme.read_text()
    for marker, body in blocks(analysis, results).items():
        pattern = rf'<!-- {marker}:start -->\n.*?\n<!-- {marker}:end -->'
        replacement = f'<!-- {marker}:start -->\n{body}\n<!-- {marker}:end -->'
        require(len(re.findall(pattern, text, re.S)) == 1, 'missing/duplicate README marker: '+marker)
        current = re.search(pattern, text, re.S).group(0)
        require(not args.check or current == replacement, 'stale README numbers: '+marker)
        text = re.sub(pattern, lambda match: replacement, text, flags=re.S)
    image_path = DEFAULT_ANALYSIS/'resumo-readme.svg'
    svg = chart(analysis, results)
    if args.check:
        require(image_path.exists() and image_path.read_text() == svg, 'stale README chart')
    else:
        args.readme.write_text(text)
        image_path.write_text(svg)
    print(f'{len(analysis.rows)} new records, 24 loads, input hashes, CPU/latency, CSV/JSON and README checked; '+
          ('no files changed.' if args.check else 'README summary and chart updated.'))


if __name__ == '__main__':
    try:
        main()
    except (OSError, ValueError, KeyError, RuntimeError, zipfile.BadZipFile) as error:
        sys.exit('read analysis rejected: '+str(error))
