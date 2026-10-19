# Tutorial manual: CrabCache e CrabPack, do primeiro comando à compressão

Este guia usa o **CrabCache v0.3.0**, publicado com o CrabPack. Você vai subir uma
instância local, gravar e recuperar dados, experimentar TTL, ativar a compressão e
conferir se os valores continuam iguais. Cada etapa indica o que executar e o que
observar.

O caminho principal usa Docker e funciona sem compilar Rust ou clonar o projeto.
Os comandos foram preparados para **macOS ou Linux, em bash/zsh**. No Windows,
execute-os no terminal do WSL com o Docker integrado.

**CrabCache** é o servidor de cache em memória. **CrabPack** é uma funcionalidade
desse servidor: você não instala outro serviço nem muda o comando `GET` para usá-la.

Nesta versão, o projeto é experimental: suporta strings, sem persistência,
replicação ou transações. JSON é armazenado como uma string; comandos de RedisJSON,
hashes, listas e sets não estão disponíveis. Ao parar ou reiniciar o servidor,
os dados do cache são perdidos, mesmo que você monte um volume Docker.

## Roteiro

1. [Preparar o terminal](#1-preparar-o-terminal).
2. [Subir o CrabCache](#2-subir-o-crabcache-sem-compressão).
3. [Usar o cache manualmente](#3-usar-o-cache-manualmente).
4. [Entender o CrabPack](#4-entender-o-que-o-crabpack-vai-fazer).
5. [Carregar 5.000 sessões](#5-preparar-um-conjunto-de-dados-para-comprimir).
6. [Conferir os dados antes da compressão](#6-conferir-todos-os-dados-antes-da-compressão).
7. [Ativar e acompanhar a compressão](#7-ativar-o-crabpack-e-acompanhar).
8. [Validar dados, TTL e memória](#8-validar-o-resultado).
9. [Desativar e reativar a compressão](#9-desativar-escrever-e-reativar).
10. [Usar em uma aplicação Node.js](#10-usar-em-uma-aplicação-nodejs-opcional).
11. [Manter a configuração no Docker Compose](#11-manter-a-configuração-no-docker-compose-opcional).
12. [Executar a partir do código Rust](#12-executar-a-partir-do-código-rust-opcional).
13. [Encerrar e repetir o laboratório](#13-encerrar-e-repetir-o-laboratório).
14. [Resolver problemas](#14-resolver-problemas).
15. [Conferir o que você aprendeu](#15-checklist-final).

Siga as etapas 1–9 na ordem. As etapas 10–12 são alternativas ou complementos.
Reserve cerca de 30–60 minutos para ler, executar e observar; downloads e compilação
podem acrescentar tempo.

## 1. Preparar o terminal

### 1.1. Ferramentas necessárias

Você precisa de Docker, `redis-cli` e Python 3. O cliente `redis-cli` fala com o
CrabCache pelo protocolo Redis; instalar esse cliente não exige iniciar um servidor Redis.

No seu Mac, abra o Docker Desktop e confira:

```bash
docker version
redis-cli --version
python3 --version
```

`docker version` deve mostrar informações de **Client e Server**. Se faltar o
Docker Desktop, siga a [instalação oficial para macOS](https://docs.docker.com/desktop/setup/install/mac-install/)
e escolha a edição correspondente ao seu processador.

Se você já tem Homebrew, instale as ferramentas que faltarem:

```bash
brew install redis python
```

Você não precisa executar `brew services start redis` para este tutorial.
O pacote e seus executáveis estão descritos na [fórmula oficial do Homebrew](https://formulae.brew.sh/formula/redis).

Em Ubuntu/Debian, com o Docker já instalado, as ferramentas de terminal podem ser
instaladas assim:

```bash
sudo apt-get update
sudo apt-get install -y redis-tools python3
```

Se os três comandos de versão funcionarem, continue. Não reinstale ferramentas
que já estão disponíveis.

### 1.2. Variáveis e pasta do laboratório

Execute tudo no **mesmo terminal**. Os comandos seguintes usam estas variáveis:

<!-- tutorial-check: setup -->
```bash
export CRABCACHE_LAB_DIR="$HOME/crabcache-lab-v030"
export CRABCACHE_LAB_PORT=7379
export CRABCACHE_LAB_CONTAINER=crabcache-tutorial-v030
export REDISCLI_AUTH='senha-local-tutorial'

mkdir -p "$CRABCACHE_LAB_DIR"

cc() {
  redis-cli -h 127.0.0.1 -p "$CRABCACHE_LAB_PORT" "$@"
}
```

`cc` é apenas um atalho para o cliente com host e porta definidos. A variável
`REDISCLI_AUTH` é lida pelo `redis-cli` para autenticar. A senha acima é um exemplo
para este laboratório local, não uma senha para publicar um serviço.

Se abrir outro terminal, execute novamente este bloco. A pasta guarda os arquivos
do exercício; ela não é um diretório de persistência do servidor.

## 2. Subir o CrabCache sem compressão

### 2.1. Baixar a versão fixa

```bash
docker pull rogerfelipensk/crabcache:v0.3.0
```

Usamos uma tag fixa para que o tutorial não mude quando uma nova versão virar `latest`.

### 2.2. Criar a instância

<!-- tutorial-check: start -->
```bash
docker run -d \
  --name "$CRABCACHE_LAB_CONTAINER" \
  -p "127.0.0.1:${CRABCACHE_LAB_PORT}:6379" \
  -e CRABCACHE_REQUIREPASS="$REDISCLI_AUTH" \
  -e CRABCACHE_THREADS=2 \
  -e CRABCACHE_MAXMEMORY=128mb \
  -e CRABCACHE_MAXMEMORY_POLICY=noeviction \
  -e CRABCACHE_COMPRESSION=false \
  rogerfelipensk/crabcache:v0.3.0
```

O host usa a porta **7379** e o container usa **6379**. A publicação em
`127.0.0.1` permite acesso pela sua máquina. O limite de 128 MB vale para a
contabilização de dados do CrabCache; ele não limita toda a RAM do processo.
Com `noeviction`, o servidor pode recusar escritas ao atingir esse limite, em vez
de remover chaves. Este conjunto de dados é pequeno para esse limite.

O comando retorna o identificador do container. Aguarde um instante e confira:

<!-- tutorial-check: connection -->
```bash
cc PING
cc INFO server
cc CONFIG GET 'compression*'
docker logs --tail 20 "$CRABCACHE_LAB_CONTAINER"
```

Resultados esperados:

- `PING`: `PONG`.
- `INFO server`: contém `crabcache_version:0.3.0`.
- Configuração: `compression` = `no`, `compression-min-idle` = `60` e
  `compression-min-size` = `64`.

O campo `redis_version` serve à compatibilidade com clientes. Para identificar
a versão deste servidor, use **`crabcache_version`**.

Se o nome do container ou a porta já estiverem ocupados, consulte a seção de
[problemas](#14-resolver-problemas) antes de continuar.

## 3. Usar o cache manualmente

### 3.1. Gravar, ler e remover

<!-- tutorial-check: basics -->
```bash
cc SET saudacao 'Olá, CrabCache!'
cc GET saudacao
cc EXISTS saudacao
cc TYPE saudacao
cc DEL saudacao
cc GET saudacao
```

Na ordem: `OK`, o texto gravado, `1`, `string`, `1` e ausência de valor.
A ausência aparece como `(nil)` no modo interativo; com saída raw, pode ser uma linha vazia.

### 3.2. Usar TTL: expiração automática

TTL é o tempo restante até uma chave expirar, em segundos. Experimente um prazo curto:

<!-- tutorial-check: ttl -->
```bash
cc SET temporario 'este valor vai expirar' EX 3
cc TTL temporario
sleep 4
cc GET temporario
cc TTL temporario
```

O primeiro TTL fica entre `0` e `3`, dependendo do tempo que você levou para
executar o comando. Depois da espera, a chave não existe e o TTL retorna `-2`.
Se você executar os comandos um a um com uma pausa longa, ela pode expirar antes
da primeira consulta.

| Resultado de `TTL` | Significado |
|---|---|
| `0` ou positivo | A chave tem prazo de expiração |
| `-1` | A chave existe e não tem prazo |
| `-2` | A chave não existe |

Para alterar uma chave preservando seu prazo, use `SET ... KEEPTTL`.
Um `SET` comum substitui o valor e remove o TTL anterior, a menos que você informe
um novo prazo com `EX`, `PX`, `EXAT` ou `PXAT`.

### 3.3. JSON, múltiplas chaves e contador

<!-- tutorial-check: strings -->
```bash
cc SET produto:42 '{"id":42,"nome":"Café","preco":19.90}' EX 300
cc --raw GET produto:42 | python3 -m json.tool

cc MSET exemplo:a 'primeiro' exemplo:b 'segundo'
cc MGET exemplo:a exemplo:b

cc SET visitas 0
cc INCR visitas
cc INCRBY visitas 5
cc GET visitas

cc DBSIZE
cc INFO stats
```

O JSON é recuperado como foi gravado. O contador passa de `0` para `1` e depois
para `6`. `INFO stats` mostra, entre outros campos, hits, misses e chaves expiradas.

### 3.4. Preparar a instância para o exercício de compressão

O comando abaixo apaga todas as chaves da **instância descartável criada neste
tutorial**, na porta 7379. Não o execute contra um cache que contenha dados que
você pretende manter.

<!-- tutorial-check: reset -->
```bash
cc FLUSHALL
cc DBSIZE
```

Resultado esperado: `OK` e `0`.

## 4. Entender o que o CrabPack vai fazer

O CrabPack trabalha em segundo plano:

1. Coleta amostras de valores e agrupa chaves pelo prefixo.
2. Treina um dicionário zstd para o grupo.
3. Aceita o dicionário se ele reduzir suficientemente as amostras de validação.
4. Comprime valores elegíveis que ficaram sem acesso pelo tempo configurado.
5. Descomprime ao ler, devolvendo os bytes originais ao cliente.

Na v0.3.0, o prefixo vai até o **primeiro `:` nos primeiros 32 bytes** da chave:

| Chave | Grupo do CrabPack |
|---|---|
| `session:42` | `session:` |
| `session:usuario:42` | `session:` |
| `product:42` | `product:` |
| `app:session:42` e `app:product:42` | Ambas em `app:` |
| Chave sem `:` nos primeiros 32 bytes | Grupo de prefixo vazio |

Agrupe valores com estrutura semelhante. Misturar produtos e sessões no mesmo
prefixo pode diminuir o ganho. Você não precisa enviar dados já comprimidos.

O treino começa ao acumular **1.000 amostras ou 256 KiB de amostras** por grupo.
Isso é uma condição de amostragem, não uma exigência de exatamente 1.000 chaves
diferentes. Este exercício usa 5.000 JSONs para tornar o benefício observável.

Um dicionário só é aceito se atingir razão de pelo menos **1,25×** nas amostras
reservadas para validação. Valores individuais também podem permanecer sem
compressão se não houver economia suficiente. Portanto, `compression:yes` não
significa que toda chave será comprimida.

Por padrão, valores menores que **64 bytes** não são candidatos e o tempo mínimo
sem acesso é **60 segundos**. Para observar o processo rapidamente, vamos usar
tempo ocioso `0` apenas neste laboratório.

## 5. Preparar um conjunto de dados para comprimir

### 5.1. Gerar os dados localmente

O bloco abaixo cria um arquivo de comandos Redis em formato RESP. Ele não grava
no servidor ainda. Copie o bloco completo, incluindo a última linha `PY`.

O gerador usa apenas a biblioteca padrão do Python. Os dados são sintéticos e
determinísticos: IDs, nomes e tokens variam, mas os campos dos JSONs são semelhantes.

<!-- tutorial-check: generate -->
```bash
python3 - <<'PY'
import hashlib
import json
import os
from pathlib import Path

directory = Path(os.environ['CRABCACHE_LAB_DIR'])
destination = directory / 'sessoes.resp'

def resp(parts):
    parts = [part if isinstance(part, bytes) else str(part).encode('utf-8')
             for part in parts]
    return (f'*{len(parts)}\r\n'.encode()
            + b''.join(f'${len(part)}\r\n'.encode() + part + b'\r\n'
                       for part in parts))

with destination.open('wb') as output:
    for i in range(5000):
        value = json.dumps({
            'user_id': i,
            'name': f'Usuário {i}',
            'email': f'user{i}@example.com',
            'roles': ['user', 'editor'] if i % 3 == 0 else ['user'],
            'locale': 'pt-BR',
            'theme': 'dark' if i % 2 == 0 else 'light',
            'last_seen': f'2026-10-{1 + i % 28:02d}T12:30:00Z',
            'cart': [{'sku': f'SKU-{i % 100}', 'qty': 1 + i % 4}],
            'csrf': hashlib.sha256(str(i).encode()).hexdigest()[:32],
        }, ensure_ascii=False, separators=(',', ':')).encode('utf-8')
        output.write(resp(['SET', f'session:{i}', value]))

print('Arquivo:', destination)
print('Chaves: 5000; de session:0 até session:4999')
print('SHA-256 do arquivo:', hashlib.sha256(destination.read_bytes()).hexdigest())
PY
```

O tamanho no RESP é contado em **bytes**, inclusive nos textos com acentos.
O SHA-256 identifica o arquivo usado no exercício; guarde-o se quiser comparar
duas execuções.

### 5.2. Carregar os dados sem compressão

<!-- tutorial-check: load -->
```bash
cc CONFIG SET compression no
cc --pipe < "$CRABCACHE_LAB_DIR/sessoes.resp"
cc DBSIZE
cc --raw GET session:42 | python3 -m json.tool
```

O cliente deve terminar a carga com **zero erros e 5.000 respostas**.
`DBSIZE` deve retornar **5000**. Se a carga apresentar erro ou outra quantidade,
resolva isso antes de medir a compressão.

Como o arquivo usa `SET` sem prazo, as sessões não expiram por padrão.
Agora adicione um TTL de uma hora à chave que usaremos para conferir a preservação
da expiração. Faça as etapas 6–9 antes de essa hora terminar.

<!-- tutorial-check: baseline -->
```bash
cc EXPIRE session:42 3600
export CRABCACHE_LAB_EXPIRES_BEFORE="$(cc --raw PEXPIRETIME session:42)"
cc TTL session:42
cc STRLEN session:42
cc MEMORY USAGE session:42

cc INFO memory > "$CRABCACHE_LAB_DIR/memoria-antes.txt"
cc INFO compression > "$CRABCACHE_LAB_DIR/compressao-antes.txt"
```

`EXPIRE` retorna `1`. `PEXPIRETIME` retorna o instante absoluto de expiração em
milissegundos desde a época Unix. Guardamos esse instante porque o TTL restante
naturalmente diminui durante o exercício.

Com o CrabPack desligado desde o início, `compressed_keys` deve ser `0`.

## 6. Conferir todos os dados antes da compressão

Vamos criar um verificador que lê os valores esperados do arquivo RESP original
e consulta as 5.000 chaves diretamente pelo protocolo. Ele compara **cada byte**,
inclusive UTF-8; não se limita a olhar o JSON de uma chave.

Ele não exige instalar biblioteca Redis para Python. O leitor RESP deste script
é pequeno e serve apenas aos comandos do laboratório.

<!-- tutorial-check: verifier -->
```bash
cat > "$CRABCACHE_LAB_DIR/verificar-sessoes.py" <<'PY'
import hashlib
import os
import socket
from pathlib import Path

def read_resp(stream):
    line = stream.readline()
    if not line:
        raise EOFError('fim de arquivo/conexão')
    if not line.endswith(b'\r\n'):
        raise RuntimeError('resposta RESP incompleta')
    kind, data = line[:1], line[1:-2]
    if kind == b'-':
        raise RuntimeError(data.decode('utf-8', errors='replace'))
    if kind == b'+':
        return data
    if kind == b':':
        return int(data)
    if kind == b'$':
        size = int(data)
        if size == -1:
            return None
        payload = stream.read(size)
        if len(payload) != size or stream.read(2) != b'\r\n':
            raise RuntimeError('valor RESP incompleto')
        return payload
    if kind == b'*':
        return [read_resp(stream) for _ in range(int(data))]
    raise RuntimeError(f'tipo RESP inesperado: {kind!r}')

def encode(parts):
    parts = [part if isinstance(part, bytes) else str(part).encode('utf-8')
             for part in parts]
    return (f'*{len(parts)}\r\n'.encode()
            + b''.join(f'${len(part)}\r\n'.encode() + part + b'\r\n'
                       for part in parts))

path = Path(os.environ['CRABCACHE_LAB_DIR']) / 'sessoes.resp'
expected = []
with path.open('rb') as source:
    while source.peek(1):
        command = read_resp(source)
        if len(command) != 3 or command[0] != b'SET':
            raise RuntimeError('comando inesperado no conjunto de dados')
        expected.append((command[1], command[2]))
if len(expected) != 5000 or len({key for key, _ in expected}) != 5000:
    raise RuntimeError('o arquivo precisa conter 5000 chaves distintas')

digest = hashlib.sha256()
with socket.create_connection(('127.0.0.1', int(os.environ['CRABCACHE_LAB_PORT'])), timeout=10) as conn:
    with conn.makefile('rb') as response:
        conn.sendall(encode(['AUTH', os.environ['REDISCLI_AUTH']]))
        if read_resp(response) != b'OK':
            raise RuntimeError('autenticação falhou')
        for key, value in expected:
            conn.sendall(encode(['GET', key]))
            actual = read_resp(response)
            if actual != value:
                raise RuntimeError(f'valor ausente ou diferente: {key.decode()}')
            # A mesma serialização distingue tamanho, chave e conteúdo no digest.
            digest.update(encode(['GET', key, actual]))

print('OK: 5000/5000 valores iguais, byte a byte.')
print('SHA-256 dos dados lidos:', digest.hexdigest())
PY

python3 "$CRABCACHE_LAB_DIR/verificar-sessoes.py"
```

Resultado esperado: `OK: 5000/5000 valores iguais, byte a byte.` e um SHA-256.
Copie o hash para suas anotações. O SHA-256 dos dados lidos tem uma serialização
própria e **não é o mesmo hash do arquivo RESP** mostrado na etapa 5; compare-o
apenas com as próximas execuções deste verificador.

Não prossiga se o script indicar dado ausente, diferente ou falha de autenticação.

## 7. Ativar o CrabPack e acompanhar

### 7.1. Ativar em tempo de execução

<!-- tutorial-check: enable -->
```bash
cc CONFIG SET compression-min-size 64
cc CONFIG SET compression-min-idle 0
cc CONFIG SET compression yes
cc CONFIG GET 'compression*'
```

Cada `CONFIG SET` retorna `OK`. A configuração deve mostrar `yes`, `0` e `64`.
Usar `0` torna todos os valores elegíveis pelo critério de tempo ocioso; o treino
e o trabalho em segundo plano ainda precisam acontecer.

### 7.2. Acompanhar manualmente

```bash
cc INFO compression
```

Execute novamente alguns segundos depois. Você deve observar:

- `compression:yes`.
- `compression_dicts` passar de `0` para pelo menos `1`.
- Um dicionário com `prefix=session:`.
- `compressed_keys` crescer, até **5000** neste conjunto sintético.
- `compression_ratio` ficar acima de `1`.

Não há um tempo fixo garantido. Se continuar em zero, confira os logs:

```bash
docker logs --tail 50 "$CRABCACHE_LAB_CONTAINER"
```

O treino aceito produz uma mensagem com `crabpack: dictionary trained`.
Para uma atualização contínua, use o laço abaixo e pare com **Ctrl+C** ao chegar
a 5.000 chaves comprimidas. Ctrl+C encerra o laço, não o container.

```bash
while true; do
  cc INFO compression
  sleep 2
done
```

Não interprete a carga como totalmente comprimida enquanto `compressed_keys` não
chegar a `5000`. Em dados próprios, 100% pode não ocorrer: alguns valores não são
bons candidatos. Para o diagnóstico, consulte a seção 14.

### 7.3. Ler os campos

| Campo | O que você está observando |
|---|---|
| `compression` | Compactação de novos candidatos habilitada ou desabilitada |
| `compression_min_idle` | Tempo mínimo sem acesso, em segundos |
| `compression_min_size` | Tamanho mínimo de valor candidato, em bytes |
| `compression_dicts` | Dicionários aceitos neste processo |
| `compression_dicts_rejected` | Treinos rejeitados por ganho insuficiente |
| `compressed_keys` | Chaves atualmente armazenadas comprimidas |
| `compressed_original_bytes` | Soma do tamanho original dos valores atualmente comprimidos |
| `compressed_stored_bytes` | Soma do tamanho do payload comprimido desses valores |
| `compression_ratio` | `compressed_original_bytes / compressed_stored_bytes` |
| `dict0:prefix=...` | Prefixo de um dicionário e razão observada durante seu treino |

O número do dicionário pode variar. A razão do treino e a razão atual do conjunto
não precisam ser iguais.

## 8. Validar o resultado

### 8.1. Conferir novamente todos os bytes

Depois de observar `compressed_keys:5000`, execute:

<!-- tutorial-check: packed-validation -->
```bash
cc DBSIZE
python3 "$CRABCACHE_LAB_DIR/verificar-sessoes.py"
cc --raw GET session:42 | python3 -m json.tool
cc STRLEN session:42
cc MEMORY USAGE session:42
```

`DBSIZE` continua em `5000`. O verificador passa e mostra o **mesmo SHA-256 dos
dados lidos** da etapa 6. O `GET` devolve o JSON original, não um bloco zstd.
`STRLEN` continua indicando o tamanho original do valor; `MEMORY USAGE` informa
a contabilização da entrada armazenada, que pode ter diminuído.

Ler uma chave comprimida **não a transforma permanentemente em uma chave sem
compressão**. O servidor descomprime para responder e mantém a representação
comprimida até uma escrita ou remoção.

### 8.2. Conferir o instante de expiração

<!-- tutorial-check: ttl-validation -->
```bash
export CRABCACHE_LAB_EXPIRES_AFTER="$(cc --raw PEXPIRETIME session:42)"

if [ "$CRABCACHE_LAB_EXPIRES_BEFORE" = "$CRABCACHE_LAB_EXPIRES_AFTER" ]; then
  echo 'OK: a compressão preservou o instante de expiração.'
else
  echo 'Confira: o instante mudou, a chave expirou ou a variável inicial não foi definida.'
fi

cc TTL session:42
```

O instante absoluto deve ser igual. O TTL restante deve estar menor do que na
etapa 5, porque o tempo passou. Igualdade do TTL restante não é o critério correto.

### 8.3. Comparar a memória contabilizada

<!-- tutorial-check: memory-validation -->
```bash
cc INFO memory > "$CRABCACHE_LAB_DIR/memoria-depois.txt"
cc INFO compression > "$CRABCACHE_LAB_DIR/compressao-depois.txt"

python3 - <<'PY'
import os
from pathlib import Path

directory = Path(os.environ['CRABCACHE_LAB_DIR'])

def fields(name):
    return dict(line.split(':', 1)
                for line in (directory / name).read_text().splitlines()
                if ':' in line and not line.startswith('#'))

before = int(fields('memoria-antes.txt')['used_memory'])
after = int(fields('memoria-depois.txt')['used_memory'])
pack = fields('compressao-depois.txt')
original = int(pack['compressed_original_bytes'])
stored = int(pack['compressed_stored_bytes'])
print(f'used_memory antes: {before:,} bytes')
print(f'used_memory depois: {after:,} bytes')
if before:
    print(f'Redução contabilizada: {(1 - after / before) * 100:.2f}%')
if original and stored:
    print(f'Razão dos payloads comprimidos: {original / stored:.2f}x')
    print(f'Redução apenas desses payloads: {(1 - stored / original) * 100:.2f}%')
print('Chaves comprimidas:', pack['compressed_keys'])
PY
```

Interprete os números com cuidado:

- `used_memory` é a contabilização interna do armazenamento, **não a RAM física
  total do processo**. Existem outras alocações, dicionários e buffers.
- Os campos `compressed_*_bytes` contam apenas os payloads dos valores comprimidos;
  não representam toda a memória ocupada por chaves, índices ou dicionários.
- Uma razão `3×` significa que esses payloads passaram a ocupar aproximadamente
  um terço dos bytes originais, uma redução de 66,7% **nesses payloads**.
- O treino usa memória temporária. Uma leitura de memória depois da carga não
  revela necessariamente o pico durante o treino.

Para observar o container como um todo:

```bash
docker stats --no-stream "$CRABCACHE_LAB_CONTAINER"
```

Não confunda esse indicador com `used_memory`, nem com os benchmarks nativos.
O exercício comprova funcionamento e integridade neste conjunto; não reproduz
automaticamente os percentuais publicados ou um desempenho de produção.

## 9. Desativar, escrever e reativar

### 9.1. Desativar novas compactações

<!-- tutorial-check: disable -->
```bash
cc CONFIG SET compression no
cc INFO compression
cc --raw GET session:42 | python3 -m json.tool
python3 "$CRABCACHE_LAB_DIR/verificar-sessoes.py"
```

Você deve observar `compression:no`, mas as 5.000 chaves já comprimidas continuam
legíveis. Desativar a opção **não descomprime todo o armazenamento** e não elimina
os dicionários necessários às leituras.

### 9.2. Regravar um valor preservando conteúdo e TTL

Este exemplo usa um JSON em uma variável shell. Isso é adequado a este conjunto
de texto, mas não é uma técnica para transportar valores binários com byte NUL.

<!-- tutorial-check: rewrite -->
```bash
CRABCACHE_LAB_VALUE42="$(cc --raw GET session:42)"
cc SET session:42 "$CRABCACHE_LAB_VALUE42" KEEPTTL
cc INFO compression
cc PEXPIRETIME session:42
python3 "$CRABCACHE_LAB_DIR/verificar-sessoes.py"
```

A escrita armazena o valor sem compressão. Com a compactação desligada,
`compressed_keys` deve cair de `5000` para **4999**. O instante de expiração
continua igual ao guardado na etapa 5, e o verificador ainda passa porque você
regravou o mesmo JSON.

### 9.3. Reativar e voltar ao tempo ocioso padrão

<!-- tutorial-check: reenable -->
```bash
cc CONFIG SET compression yes
```

Ainda com tempo ocioso `0`, acompanhe até a chave voltar a ser compactada:

```bash
cc INFO compression
```

Depois de observar `compressed_keys:5000`, volte ao padrão:

<!-- tutorial-check: default-idle -->
```bash
cc CONFIG SET compression-min-idle 60
cc CONFIG GET 'compression*'
```

A mudança do tempo ocioso não desfaz compactações anteriores. Para experimentar
o comportamento de 60 segundos, mantenha a opção habilitada, regrave o valor da
etapa 9.2 e deixe a chave sem leituras/escritas por **pelo menos 60 segundos, mais
o tempo de varredura**. Consulte `INFO compression` enquanto aguarda; `GET` na
chave pode atualizar sua informação de acesso e adiar a elegibilidade.

**As mudanças com `CONFIG SET` valem apenas para o processo atual.** Neste container,
`docker restart` apaga os dados e volta a `CRABCACHE_COMPRESSION=false`, porque foi
assim que ele foi criado. A próxima seção de configuração mostra como iniciar
sempre com a opção habilitada.

## 10. Usar em uma aplicação Node.js (opcional)

O cliente continua usando `SET` e `GET`. A aplicação não precisa saber se um valor
está comprimido. Este exemplo usa `node-redis` **4.7.1**, com versão fixada para
reproduzir a API do exemplo, e pressupõe que o Node.js com suporte a módulos ESM
está disponível.

Confira `node --version` e `npm --version`. Se ainda não tiver Node.js, use um
instalador LTS da [página oficial de downloads](https://nodejs.org/en/download),
abra novamente o terminal e refaça as variáveis da etapa 1. Execute no mesmo terminal:

<!-- tutorial-check: node-setup -->
```bash
mkdir -p "$CRABCACHE_LAB_DIR/node"
cd "$CRABCACHE_LAB_DIR/node"
npm init -y
npm install --save-exact redis@4.7.1
```

Crie uma aplicação que simula uma consulta lenta e armazena o resultado por 30 segundos:

<!-- tutorial-check: node-app -->
```bash
cat > "$CRABCACHE_LAB_DIR/node/app.mjs" <<'JS'
import { createClient } from 'redis';

const client = createClient({
  socket: {
    host: '127.0.0.1',
    port: Number(process.env.CRABCACHE_LAB_PORT),
  },
  password: process.env.REDISCLI_AUTH,
});
client.on('error', error => console.error('Erro no cache:', error.message));
await client.connect();

async function buscarProduto(id) {
  const key = `produto:${id}`;
  const cached = await client.get(key);
  if (cached !== null) {
    console.log('HIT: encontrado no cache');
    return JSON.parse(cached);
  }

  console.log('MISS: consultando a origem simulada');
  await new Promise(resolve => setTimeout(resolve, 200));
  const product = { id, nome: 'Café', preco: 19.9 };
  await client.set(key, JSON.stringify(product), { EX: 30 });
  return product;
}

try {
  // Limpa apenas a chave deste exemplo para produzir MISS seguido de HIT.
  await client.del('produto:42');
  console.log(await buscarProduto(42));
  console.log(await buscarProduto(42));
  console.log('TTL:', await client.ttl('produto:42'));
} finally {
  await client.quit();
}
JS

node "$CRABCACHE_LAB_DIR/node/app.mjs"
```

Você deve ver primeiro `MISS`, depois `HIT`, o mesmo produto nas duas respostas
e um TTL próximo de `30`. Esse padrão é chamado de **cache-aside**: consulta o
cache, busca na origem se faltar e guarda o resultado com prazo.

A origem continua responsável pelo dado correto; sua aplicação também precisa
decidir como invalidar o cache quando o produto mudar. Em uma integração real,
trate indisponibilidade do cache e obtenha novamente o dado da origem.

Uma chave curta isolada como essa não demonstra o CrabPack. O lote de sessões
das etapas anteriores é o exercício de compressão; aqui verificamos a integração
com um cliente comum. O arquivo `.mjs` permite usar módulos sem alterar o `package.json`.

## 11. Manter a configuração no Docker Compose (opcional)

Este exemplo cria **outra instância**, na porta 7380. Os dados do laboratório na
porta 7379 não são copiados. Ele inicia com o CrabPack habilitado e tempo ocioso de
60 segundos.

<!-- tutorial-check: compose-file -->
```bash
cat > "$CRABCACHE_LAB_DIR/compose.yaml" <<'YAML'
services:
  crabcache:
    image: rogerfelipensk/crabcache:v0.3.0
    ports:
      - "127.0.0.1:7380:6379"
    environment:
      CRABCACHE_REQUIREPASS: "${REDISCLI_AUTH:?Defina REDISCLI_AUTH no terminal}"
      CRABCACHE_COMPRESSION: "true"
      CRABCACHE_COMPRESSION_MIN_IDLE: "60"
      CRABCACHE_COMPRESSION_MIN_SIZE: "64"
      CRABCACHE_MAXMEMORY: "128mb"
      CRABCACHE_MAXMEMORY_POLICY: "allkeys-lru"
      CRABCACHE_THREADS: "2"
YAML

docker compose -p crabcache-tutorial-compose -f "$CRABCACHE_LAB_DIR/compose.yaml" up -d
redis-cli -h 127.0.0.1 -p 7380 PING
redis-cli -h 127.0.0.1 -p 7380 CONFIG GET 'compression*'
```

O `PING` deve responder `PONG`, com compressão `yes`, tempo ocioso `60` e tamanho
mínimo `64`. Aqui usamos `allkeys-lru`: o servidor pode remover chaves para cumprir
o limite de dados. Isso é diferente do `noeviction` usado na validação do conjunto.

Para encerrar apenas essa segunda instância:

<!-- tutorial-check: compose-stop -->
```bash
docker compose -p crabcache-tutorial-compose -f "$CRABCACHE_LAB_DIR/compose.yaml" down
```

O arquivo mantém a **configuração de inicialização**, não os dados. Reiniciar ou
recriar o serviço perde o cache. Não há persistência RDB/AOF na v0.3.0; adicionar
um volume não muda isso.

## 12. Executar a partir do código Rust (opcional)

Este caminho requer Git, Rust **1.99 ou superior**, Cargo e as ferramentas nativas
de compilação da sua plataforma. Confira:

```bash
git --version
rustc --version
cargo --version
```

Se faltar Rust, a [instalação oficial por rustup](https://rust-lang.org/tools/install/)
para macOS/Linux usa:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Siga as instruções do instalador e abra outro terminal para carregar o PATH.
Depois refaça as variáveis da etapa 1. No Mac, se faltarem as ferramentas de
compilação, instale as Command Line Tools com `xcode-select --install`.
Em Ubuntu/Debian, use `sudo apt-get install -y build-essential git`.

Se já usa este repositório localmente, você pode trabalhar na pasta do projeto.
Para compilar exatamente a versão deste tutorial sem mudar sua branch atual,
faça um clone separado:

```bash
git clone --branch v0.3.0 --depth 1 \
  https://github.com/RogerFelipeNsk/crabcache.git \
  "$CRABCACHE_LAB_DIR/source"

cd "$CRABCACHE_LAB_DIR/source"
cargo build --release --locked --bin crabcache --example dataset
target/release/crabcache --version
target/release/crabcache --help
```

O clone de uma tag pode informar `detached HEAD`: é esperado para consultar e
compilar essa versão. Se a pasta `source` já existir, use-a ou escolha outra pasta.

Inicie em **um segundo terminal**, na pasta do clone. A porta 7381 permite coexistir
com as instâncias anteriores:

```bash
cd "$HOME/crabcache-lab-v030/source"
export CRABCACHE_REQUIREPASS='senha-local-tutorial'
target/release/crabcache \
  --bind 127.0.0.1 \
  --port 7381 \
  --threads 2 \
  --maxmemory 128mb \
  --compression \
  --compression-min-idle 0 \
  --compression-min-size 64
```

O servidor fica em primeiro plano. Para encerrá-lo, use Ctrl+C nesse terminal.
No terminal original, teste e carregue o gerador já incluído no projeto:

```bash
export REDISCLI_AUTH='senha-local-tutorial'
redis-cli -h 127.0.0.1 -p 7381 PING
"$CRABCACHE_LAB_DIR/source/target/release/examples/dataset" session 5000 \
  | redis-cli -h 127.0.0.1 -p 7381 --pipe
redis-cli -h 127.0.0.1 -p 7381 INFO compression
redis-cli -h 127.0.0.1 -p 7381 --raw GET session:42 | python3 -m json.tool
```

Esse gerador tem conteúdo diferente do script Python da etapa 5. Não compare seus
valores com o arquivo `sessoes.resp` anterior. Para repetir exatamente a mesma
validação nessa instância, carregue o arquivo da etapa 5, use compressão inicialmente
desligada e defina `CRABCACHE_LAB_PORT=7381` antes de executar o verificador.

Para aprofundar a validação do código:

```bash
cd "$CRABCACHE_LAB_DIR/source"
CRABCACHE_REQUIRE_REDIS=1 cargo test --release --locked
```

Esse último comando precisa de `redis-server` instalado no host como referência.
Se estiver disponível, os testes iniciam instâncias próprias; não é necessário
ter um serviço Redis permanente rodando. Com `CRABCACHE_REQUIRE_REDIS=1`, faltar
esse binário causa falha em vez de ignorar os testes diferenciais.

## 13. Encerrar e repetir o laboratório

### 13.1. Parar o servidor Docker

<!-- tutorial-check: stop -->
```bash
docker stop "$CRABCACHE_LAB_CONTAINER"
```

Isso interrompe o servidor e perde os dados em memória. Para voltar a iniciar o
container existente:

```bash
docker start "$CRABCACHE_LAB_CONTAINER"
cc PING
cc DBSIZE
cc CONFIG GET compression
```

Sem outra aplicação gravando, `DBSIZE` retorna `0`. A compressão volta a `no`
porque essa foi a configuração de criação na etapa 2.

### 13.2. Remover apenas o container deste guia

Depois de pará-lo:

<!-- tutorial-check: remove -->
```bash
docker rm "$CRABCACHE_LAB_CONTAINER"
unset REDISCLI_AUTH
```

Os arquivos de referência e resultados continuam em `$CRABCACHE_LAB_DIR`.
Para repetir, refaça a configuração do terminal, a criação do container e a carga.
Não é necessário apagar a pasta: o gerador e as capturas podem sobrescrever seus
próprios arquivos. Se quiser preservar uma rodada, copie a pasta antes de repetir.

`FLUSHALL` apaga as chaves, mas **não zera os dicionários treinados** na v0.3.0.
Para observar novamente o treino desde o início, reinicie ou recrie a instância
descartável e reaplique a configuração desejada.

## 14. Resolver problemas

| Sintoma | Verificação e ação |
|---|---|
| Docker não mostra Server / não conecta ao daemon | Abra o Docker Desktop e aguarde sua inicialização. Confira `docker version` novamente. |
| `redis-cli: command not found` | Instale o cliente conforme a etapa 1 e confira seu PATH. |
| Nome do container já existe | Consulte `docker ps -a --filter "name=$CRABCACHE_LAB_CONTAINER"`. Se for a sua instância deste guia, use `docker start` ou pare/remova apenas ela para repetir. |
| Porta 7379 ocupada | Identifique quem usa a porta, ou escolha outra em `CRABCACHE_LAB_PORT` **antes de criar o container**. Recrie o container do laboratório para mudar o mapeamento. |
| `Connection refused` | Confira container, logs, porta e se a variável aponta para a instância correta: `docker ps`, `docker logs --tail 50 "$CRABCACHE_LAB_CONTAINER"`, `cc PING`. |
| `NOAUTH` / senha incorreta | Refaça `export REDISCLI_AUTH='senha-local-tutorial'` no terminal. A senha precisa ser a mesma usada ao criar o container. |
| `compression:yes` e zero chaves comprimidas logo depois | O treino e a varredura são graduais. Aguarde, confira dicionários/logs e confirme que carregou os 5.000 valores. |
| Nenhum dicionário aceito | Confira tamanho, prefixo, amostras e logs de treino. Dados pouco semelhantes ou já comprimidos podem não oferecer ganho. |
| `compression_dicts_rejected` cresce | O treino não atingiu o ganho mínimo. Após uma rejeição por ganho insuficiente, esse prefixo fica sem nova amostragem por cerca de 600 segundos. Use dados adequados ou uma instância nova para repetir o treino. |
| Dicionário existe, mas `compressed_keys` está abaixo do esperado | Confira tempo ocioso, acessos frequentes, tamanhos e se os valores ainda existem. Alguns valores podem ser recusados por falta de economia. |
| Verificador encontrou `session:42` ausente | Ela pode ter atingido o TTL de uma hora. Confira `TTL`, recarregue o conjunto e repita as etapas de captura/validação. |
| Verificador encontrou um valor diferente | Confira se você alterou a chave, carregou outro gerador ou apontou para outra porta. Não considere a validação aprovada até explicar a diferença. |
| Erro de limite de memória | Confira `INFO memory`, limite e política. Com `noeviction`, escritas podem ser recusadas; com eviction, chaves podem desaparecer. |
| Dados desapareceram após restart | É o comportamento desta versão, que mantém dados apenas em memória. Recarregue a partir da origem. |
| Compressão voltou a `no` após restart | `CONFIG SET` não modifica as variáveis originais do container. Recrie com `CRABCACHE_COMPRESSION=true` ou use a etapa 11. |
| `docker exec ... sh` não funciona | A imagem é distroless e não inclui shell nem `redis-cli`. Use o cliente no host e `docker logs`. |
| `SELECT 1`, `HSET`, `MULTI` ou RedisJSON falham | A v0.3.0 suporta apenas o banco 0 e o conjunto de comandos de strings/chaves documentado no README. |

Para conferir estado geral:

```bash
cc INFO server
cc INFO memory
cc INFO stats
cc INFO compression
cc INFO keyspace
```

Evite alterar várias condições ao mesmo tempo: primeiro conexão, depois carga,
depois treino, depois compactação e finalmente leitura.

## 15. Checklist final

Você concluiu o exercício principal quando conseguiu:

- [ ] Receber `PONG` da instância na porta 7379 e confirmar `crabcache_version:0.3.0`.
- [ ] Executar `SET`, `GET`, `DEL`, `MGET`, `INCR` e observar uma expiração por TTL.
- [ ] Carregar 5.000 chaves com zero erros e conferir `DBSIZE`.
- [ ] Validar os 5.000 valores antes da compressão.
- [ ] Ativar o CrabPack, observar um dicionário de `session:` e 5.000 chaves comprimidas.
- [ ] Validar todos os valores novamente com o mesmo hash dos dados lidos.
- [ ] Confirmar que o instante absoluto de expiração não mudou.
- [ ] Distinguir economia de payload, memória contabilizada e RAM do processo.
- [ ] Desativar a compactação e continuar lendo os valores já comprimidos.
- [ ] Regravar um valor com `KEEPTTL`, ver a contagem cair e conferir a integridade.
- [ ] Entender que restart perde os dados e restaura a configuração de inicialização.

### Próximos exercícios com seus próprios dados

Use uma instância descartável e valores que representem seu cache: tamanhos,
prefixos, frequência de leitura/escrita e TTLs. Primeiro confira integridade;
depois meça memória e latência com e sem compressão, mantendo as condições iguais.
Desligar `compression` em uma instância já comprimida não produz uma comparação
sem compressão: para isso, use uma instância nova, com os mesmos valores carregados
e compactação desligada desde o início.

Descomprimir custa CPU. O ganho de memória não garante ganho de throughput. Para
comparações quantitativas, faça repetições, guarde os resultados e confira hits,
misses, erros e bytes retornados. Veja os procedimentos e limites em
[BENCHMARKS.md](BENCHMARKS.md).

Na fase 1, há no máximo 64 dicionários aceitos por processo e eles não são
retreinados automaticamente. Evite criar um prefixo distinto para cada usuário:
prefira um grupo por tipo de dado, como `session:` ou `product:`.

### Referências

- [README: configuração e comandos suportados](../README.md#configuração).
- [Arquitetura e implementação do CrabPack](ARCHITECTURE.md#compressão-crabpack).
- [Benchmarks auditados e limitações](BENCHMARKS.md).
- [Notas da release v0.3.0](releases/v0.3.0.md).
- [Docker Desktop: instalação oficial para macOS](https://docs.docker.com/desktop/setup/install/mac-install/).
- [Redis CLI: autenticação, saída raw e uso do cliente](https://redis.io/docs/latest/develop/tools/cli/).
- [Cliente Node.js usado no exemplo: node-redis 4.7.1](https://github.com/redis/node-redis/blob/redis%404.7.1/README.md).

Os comportamentos específicos do CrabCache deste guia foram conferidos no código
da v0.3.0 e os exemplos principais foram executados contra sua imagem publicada.
