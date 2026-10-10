# Evidências da auditoria de 09/10/2026

99 registros completos, com três repetições por cenário: 12 de uma thread de I/O, 18 de memória
com valores fixos, 27 de memória com JSON e 18 de GET com JSON, mais 24 de configuração padrão.

Cada diretório contém `summary.json` (metadados e registros por rodada), `*.dataset.json`
(contagem, tamanho em bytes e hash do conjunto), os resultados numéricos de memtier/redis-benchmark
e `traces.zip` (comandos, stdout/stderr, confirmação de carga e logs dos servidores).
Os arquivos numéricos são as saídas das ferramentas; os resumos incluem os valores medidos de CPU
cumulativa e memória física. A pasta `tests/` preserva logs e comandos das validações Rust/Python.

A fase de memória com JSON foi preservada integralmente. A fase de GET foi reexecutada depois de
corrigir a comparação entre operações executadas no servidor e respostas contabilizadas no cliente
no encerramento de um pipeline. Todas as três repetições da fase reexecutada estão incluídas, sem
seleção por desempenho. A comparação de uma thread também foi reexecutada com o cálculo final de
operações por segundo de CPU. As fases estão identificadas nos metadados da compressão.

O código do servidor não foi alterado nesta auditoria. O commit-base e o hash do binário estão em
cada resumo; `git_dirty: true` reflete as mudanças nos testes, scripts e documentação.
Os arquivos RESP grandes são reconstruíveis pelo gerador determinístico e não foram armazenados.

```bash
python3 scripts/validate-benchmark-results.py docs/benchmark-results/2026-10-09
python3 scripts/report-benchmarks.py
python3 scripts/gen-charts.py docs/img
```

Os comandos são executados a partir da raiz do repositório. A validação lê os ZIPs diretamente,
confere cargas, repetições, resumos e valores derivados. Limitações e faixas de variação aparecem
em [BENCHMARKS.md](../../BENCHMARKS.md).
