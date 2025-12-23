# CrabCache API Documentation

## Visão Geral

CrabCache é um sistema de cache moderno escrito em Rust que oferece uma API simples baseada em protocolo de texto sobre TCP. Este documento descreve todos os comandos disponíveis e como utilizá-los.

## Conexão

- **Protocolo**: TCP
- **Porta padrão**: 7000
- **Formato**: Texto simples terminado em `\r\n`

### Exemplo de Conexão

```bash
# Via telnet
telnet localhost 7000

# Via netcat
nc localhost 7000

# Via Docker
docker run -p 7004:7000 crabcache:latest
nc localhost 7004
```

## Comandos Disponíveis

### 1. PING - Health Check

Verifica se o servidor está respondendo.

**Sintaxe**: `PING`

**Exemplo**:
```
> PING
< PONG
```

**Respostas**:
- `PONG` - Servidor funcionando normalmente

---

### 2. PUT - Armazenar Valor

Armazena uma chave-valor no cache, opcionalmente com TTL.

**Sintaxe**: `PUT <key> <value> [ttl_seconds]`

**Exemplos**:
```bash
# Armazenar sem TTL
> PUT user:123 john_doe
< OK

# Armazenar com TTL de 1 hora (3600 segundos)
> PUT session:abc123 user_session_data 3600
< OK

# Armazenar dados JSON
> PUT user:profile:123 {"name":"John","age":30}
< OK
```

**Respostas**:
- `OK` - Valor armazenado com sucesso
- `ERROR: Memory limit exceeded` - Limite de memória excedido

---

### 3. GET - Recuperar Valor

Recupera o valor de uma chave do cache.

**Sintaxe**: `GET <key>`

**Exemplos**:
```bash
# Recuperar valor existente
> GET user:123
< john_doe

# Recuperar valor inexistente ou expirado
> GET nonexistent:key
< NULL

# Recuperar dados JSON
> GET user:profile:123
< {"name":"John","age":30}
```

**Respostas**:
- `<value>` - Valor da chave
- `NULL` - Chave não encontrada ou expirada

---

### 4. DEL - Remover Chave

Remove uma chave do cache.

**Sintaxe**: `DEL <key>`

**Exemplos**:
```bash
# Remover chave existente
> DEL user:123
< OK

# Remover chave inexistente
> DEL nonexistent:key
< NULL
```

**Respostas**:
- `OK` - Chave removida com sucesso
- `NULL` - Chave não existia

---

### 5. EXPIRE - Definir TTL

Define ou atualiza o TTL de uma chave existente.

**Sintaxe**: `EXPIRE <key> <ttl_seconds>`

**Exemplos**:
```bash
# TTL de 1 hora (3600 segundos)
> EXPIRE user:123 3600
< OK

# TTL de 5 minutos (300 segundos)
> EXPIRE session:abc123 300
< OK

# TTL de 1 dia (86400 segundos)
> EXPIRE cache:data 86400
< OK

# Chave inexistente
> EXPIRE nonexistent:key 3600
< NULL
```

**Respostas**:
- `OK` - TTL atualizado com sucesso
- `NULL` - Chave não encontrada

---

### 6. STATS - Estatísticas

Obtém estatísticas detalhadas do servidor.

**Sintaxe**: `STATS`

**Exemplo**:
```bash
> STATS
< STATS: shard_0: 10 keys, 1024B/1073741824B, shard_1: 15 keys, 2048B/1073741824B, shard_2: 5 keys, 512B/1073741824B, total: 30 keys, 3584B/3221225472B memory
```

**Informações Retornadas**:
- Número de chaves por shard
- Uso de memória por shard (usado/máximo)
- Total de chaves e memória no sistema
- Chaves com TTL ativo (quando aplicável)

## Padrões de Uso

### 1. Cache de Sessão

```bash
# Criar sessão com TTL de 30 minutos
PUT session:user123 {"user_id":123,"role":"admin"} 1800

# Verificar sessão
GET session:user123

# Renovar sessão por mais 30 minutos
EXPIRE session:user123 1800

# Logout (remover sessão)
DEL session:user123
```

### 2. Cache de Dados

```bash
# Cache de perfil de usuário
PUT user:profile:123 {"name":"John","email":"john@example.com"}

# Cache temporário com TTL de 1 hora
PUT temp:calculation:abc {"result":42,"timestamp":1703030400} 3600

# Recuperar dados
GET user:profile:123
GET temp:calculation:abc
```

### 3. Cache de API

```bash
# Cache de resposta de API com TTL de 10 minutos
PUT api:weather:london {"temp":15,"humidity":80} 600

# Cache de dados que mudam raramente (1 dia)
PUT api:config:app {"version":"1.0","features":["cache","ttl"]} 86400
```

## Convenções de Nomenclatura

### Recomendações para Chaves

- Use `:` como separador hierárquico: `user:profile:123`
- Prefixos por tipo: `session:`, `cache:`, `temp:`
- IDs específicos: `user:123`, `order:456`
- Dados temporários: `temp:calculation:abc`

### Exemplos de Padrões

```bash
# Usuários
user:123                    # Dados básicos do usuário
user:profile:123           # Perfil completo
user:settings:123          # Configurações

# Sessões
session:abc123             # Dados da sessão
session:token:xyz789       # Token de autenticação

# Cache de API
api:weather:london         # Cache de API externa
api:rates:usd_brl         # Taxa de câmbio

# Dados temporários
temp:upload:file123        # Upload temporário
temp:calculation:hash456   # Cálculo temporário
```

## Códigos de Resposta

| Resposta | Significado |
|----------|-------------|
| `OK` | Operação realizada com sucesso |
| `PONG` | Resposta ao comando PING |
| `NULL` | Chave não encontrada ou expirada |
| `<value>` | Valor da chave solicitada |
| `STATS: ...` | Estatísticas do servidor |
| `ERROR: <message>` | Erro na operação |

## Limites e Características

### Limites Técnicos

- **Tamanho máximo da chave**: Limitado pela memória disponível
- **Tamanho máximo do valor**: Limitado pela memória disponível
- **TTL mínimo**: 1 segundo
- **TTL máximo**: 2^64 segundos (praticamente ilimitado)
- **Número de chaves**: Limitado pela memória configurada por shard

### Características de Performance

- **Sharding automático**: Distribuição por hash da chave
- **Operações O(1)**: GET, PUT, DEL são operações de tempo constante
- **TTL eficiente**: Sistema de TTL wheel para expiração rápida
- **Multi-core**: Paralelismo real com shards independentes
- **Memória eficiente**: Layout binário compacto com varint encoding

## Monitoramento

### Comando STATS Detalhado

O comando `STATS` retorna informações valiosas para monitoramento:

```bash
> STATS
< STATS: shard_0: 150 keys, 15360B/1073741824B, shard_1: 200 keys, 20480B/1073741824B, shard_2: 100 keys, 10240B/1073741824B, total: 450 keys, 46080B/3221225472B memory
```

**Interpretação**:
- `shard_X`: ID do shard
- `Y keys`: Número de chaves no shard
- `ZB/WB`: Memória usada/máxima em bytes
- `total`: Agregado de todos os shards

### Métricas Importantes

- **Taxa de utilização de memória**: `usado/máximo` por shard
- **Distribuição de chaves**: Verificar se está balanceada entre shards
- **Crescimento**: Monitorar crescimento ao longo do tempo

## Integração com Ferramentas

### Insomnia/Postman

1. Importe a coleção: `docs/insomnia-collection.json`
2. Configure o ambiente (Local ou Docker)
3. Execute os comandos de exemplo

### Swagger/OpenAPI

- Especificação disponível em: `docs/api-spec.yaml`
- Visualize em: https://editor.swagger.io/

### Clientes Programáticos

#### Python
```python
import socket

def crabcache_command(host, port, command):
    sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    sock.connect((host, port))
    sock.send(f"{command}\r\n".encode())
    response = sock.recv(1024).decode().strip()
    sock.close()
    return response

# Exemplo de uso
result = crabcache_command("localhost", 7000, "PUT mykey myvalue")
print(result)  # OK
```

#### Node.js
```javascript
const net = require('net');

function crabcacheCommand(host, port, command) {
    return new Promise((resolve, reject) => {
        const client = net.createConnection(port, host, () => {
            client.write(`${command}\r\n`);
        });
        
        client.on('data', (data) => {
            resolve(data.toString().trim());
            client.end();
        });
        
        client.on('error', reject);
    });
}

// Exemplo de uso
crabcacheCommand('localhost', 7000, 'GET mykey')
    .then(result => console.log(result));
```

## Troubleshooting

### Problemas Comuns

1. **Conexão recusada**
   - Verifique se o servidor está rodando
   - Confirme a porta (7000 padrão)
   - Para Docker, use a porta mapeada

2. **Comando não reconhecido**
   - Verifique a sintaxe do comando
   - Certifique-se de terminar com `\r\n`
   - Comandos são case-sensitive

3. **Memory limit exceeded**
   - Aumente `max_memory_per_shard` na configuração
   - Implemente limpeza de chaves antigas
   - Use TTL para expiração automática

4. **Chave não encontrada (NULL)**
   - Verifique se a chave foi armazenada corretamente
   - Pode ter expirado (TTL)
   - Verifique se não foi removida (DEL)

### Logs e Debug

```bash
# Rodar com logs detalhados
RUST_LOG=debug cargo run

# Docker com logs
docker run -e RUST_LOG=info -p 7000:7000 crabcache:latest
```