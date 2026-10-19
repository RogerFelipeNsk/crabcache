# Evidência do ensaio complementar de leituras — 11/10/2026

CrabCache 0.3.0 nativo no M1 Pro/macOS: 10.000 chaves por conjunto, três rodadas de
4 segundos, uma thread de I/O, sem afinidade a núcleo. Cliente e servidor compartilham
a máquina, por loopback. A auditoria de [09/10/2026](../2026-10-09) permanece separada.

Os quatro conjuntos são sessões, produtos, respostas pequenas de API e páginas de
API com 40 itens. O ensaio compara CrabPack desligado/ligado; quando ligado, todas
as chaves já estavam comprimidas antes de medir. A medição de GET cobre uma conexão
sem pipeline e 48 conexões com pipeline 16. Há também uma medição de objeto pronto
no cliente Python, incluindo GET, verificações de conteúdo e interpretação de JSON.

## Arquivos

- `summary.json`: 96 registros completos, configuração, hashes e referências às cargas.
- `*.memtier.json`: 48 saídas numéricas originais de GET, incluindo contagens e latências.
- `*.dataset.json`: tamanho médio, hash e descrição de cada conjunto sintético.
- `traces.zip`: quatro conjuntos RESP completos e comandos/logs originais das 24 instâncias,
  incluindo as 24 cargas, todas com zero erros e 10.000 respostas.
- `memory-timeline.json`: observação adicional pareada de sessões até 20 segundos
  após a compactação; uma rodada, separada das três rodadas de GET.

Para publicação, o prefixo absoluto da pasta do repositório nos comandos e logs
de `traces.zip` foi substituído por `.`. Os conjuntos RESP, resultados numéricos,
contagens, tempos e demais conteúdos foram preservados; os hashes dos conjuntos
continuam verificáveis pelo comando abaixo.

Todos os registros completos dos quatro conjuntos foram preservados, sem seleção
por desempenho. A comparação foi delimitada a compressão após esclarecimento do
pedido; dados de outros cenários exploratórios não fazem parte deste ensaio.

As latências médias são recalculadas por soma das latências / soma das contagens.
CPU/GET usa os segundos de CPU registrados / contagem do cliente. A conferência
verifica essas fórmulas; ela não reconstrói amostras de CPU nem tempos individuais
do cliente que não foram registrados. A observação de memória não mede o pico
do treino nem estabelece o consumo em prazos maiores.

## Conferir sem iniciar servidores

Na raiz do repositório:

```bash
python3 scripts/report-read-analysis.py --check
```

O comando usa apenas a biblioteca padrão do Python e não altera arquivos. Confere
os conjuntos completos pelo hash e comprimentos RESP, cargas, repetições, JSONs do
memtier, contagens, misses, cobertura de compressão, fórmulas numéricas e CSV/JSON
agregados. Também exige que o resumo e o gráfico compacto do README coincidam com
a geração a partir das medições. O CI executa esse comando.

Para regenerar o resumo e o gráfico compacto:

```bash
python3 scripts/report-read-analysis.py
```

Esse comando atualiza somente os blocos identificados no README e
`docs/analises/leituras-crabpack-2026-10-11/resumo-readme.svg`. Ele não produz
medições novas.

Para regenerar o relatório e os gráficos completos a partir destes arquivos:

```bash
python3 -m venv --system-site-packages target/analysis-venv
target/analysis-venv/bin/python -m pip install matplotlib
target/analysis-venv/bin/python docs/analises/leituras-crabpack-2026-10-11/graficos.py docs/benchmark-results/2026-10-11-read-analysis
python3 scripts/report-read-analysis.py --check
```

O gerador lê os conjuntos RESP diretamente do ZIP. Não é necessário extraí-los.

## Registrar uma nova medição

1. Execute `medir.py` e `memoria.py`, conforme o
   [relatório completo](../../analises/leituras-crabpack-2026-10-11/RELATORIO.md#reproduzir-e-conferir),
   usando o mesmo número de chaves nos dois. Cada execução cria seu próprio diretório.
2. Guarde configuração, versão, hash do binário, conjuntos, todas as rodadas e saídas
   brutas numa pasta com uma nova data. Registre os conjuntos RESP no ZIP e mantenha
   os JSONs do memtier acessíveis; copie a linha do tempo de memória como `memory-timeline.json`.
3. Regenere o relatório e seus CSV/JSON usando os novos diretórios. Para publicar
   um novo ensaio, atualize as referências de data/pasta do gerador de resumo e do README.
   Não sobrescreva um ensaio antigo nem misture ambientes diferentes.
4. Execute `report-read-analysis.py --check` e os checks da auditoria anterior.
   Divergências precisam ser corrigidas antes de publicar.

Os números representam **a última medição documentada**, não o desempenho garantido
de qualquer commit ou ambiente. Uma mudança no código pode exigir um novo ensaio;
os checks de consistência não substituem essa medição.

Veja [o relatório](../../analises/leituras-crabpack-2026-10-11/RELATORIO.md),
[os dados agregados](../../analises/leituras-crabpack-2026-10-11/dados-conferidos.csv)
e [o resumo no README](../../../README.md#cpu-latência-e-memória-do-crabpack).
