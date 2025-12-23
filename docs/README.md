# CrabCache Documentation

Esta pasta contém toda a documentação da API do CrabCache.

## 📁 Arquivos Disponíveis

### 📋 [API.md](./API.md)
Documentação completa da API com exemplos práticos, padrões de uso e troubleshooting.

### 🔧 [api-spec.yaml](./api-spec.yaml)
Especificação OpenAPI 3.0 (Swagger) da API do CrabCache.

**Como usar**:
1. Abra https://editor.swagger.io/
2. Cole o conteúdo do arquivo `api-spec.yaml`
3. Visualize a documentação interativa

### 📱 [insomnia-collection.json](./insomnia-collection.json)
Coleção completa para Insomnia/Postman com todos os comandos pré-configurados.

**Como importar no Insomnia**:
1. Abra o Insomnia
2. Clique em "Import/Export" → "Import Data"
3. Selecione "From File"
4. Escolha o arquivo `insomnia-collection.json`
5. A coleção "CrabCache API" será criada com:
   - ✅ Todos os comandos (PING, PUT, GET, DEL, EXPIRE, STATS)
   - ✅ Exemplos práticos
   - ✅ Ambientes pré-configurados (Local e Docker)
   - ✅ Organizados por categoria

**Como importar no Postman**:
1. Abra o Postman
2. Clique em "Import"
3. Arraste o arquivo `insomnia-collection.json` ou clique "Upload Files"
4. A coleção será importada automaticamente

## 🚀 Como Testar

### 1. Servidor Local
```bash
# Iniciar o servidor
cd crabcache
cargo run

# Em outro terminal, testar
echo "PING" | nc localhost 7000
```

### 2. Docker
```bash
# Build e run
cd crabcache
docker build -t crabcache:latest .
docker run -d -p 7000:7000 -e RUST_LOG=info crabcache:latest

# Testar
echo "PING" | nc localhost 7000
```

### 3. Com Insomnia/Postman
1. Importe a coleção
2. Selecione o ambiente apropriado:
   - **Local Development**: `localhost:7000`
   - **Docker Container**: `localhost:7004` (ou sua porta mapeada)
3. Execute os comandos na ordem sugerida

## 📊 Comandos Disponíveis

| Comando | Descrição | Exemplo |
|---------|-----------|---------|
| `PING` | Health check | `PING` → `PONG` |
| `PUT` | Armazenar valor | `PUT key value [ttl]` |
| `GET` | Recuperar valor | `GET key` |
| `DEL` | Remover chave | `DEL key` |
| `EXPIRE` | Definir TTL | `EXPIRE key seconds` |
| `STATS` | Estatísticas | `STATS` |

## 🔍 Exemplos Rápidos

### Operações Básicas
```bash
# Armazenar
PUT user:123 john_doe

# Recuperar
GET user:123
# → john_doe

# Remover
DEL user:123
# → OK
```

### Com TTL
```bash
# Armazenar com TTL de 1 hora
PUT session:abc123 user_data 3600

# Verificar
GET session:abc123
# → user_data

# Atualizar TTL para 30 minutos
EXPIRE session:abc123 1800
```

### Monitoramento
```bash
# Estatísticas do servidor
STATS
# → STATS: shard_0: 10 keys, 1024B/1073741824B, shard_1: 15 keys, 2048B/1073741824B, total: 25 keys, 3072B/2147483648B memory
```

## 🛠️ Ferramentas Recomendadas

### Para Desenvolvimento
- **Insomnia**: Interface gráfica amigável
- **Postman**: Alternativa popular
- **curl/nc**: Testes rápidos via linha de comando

### Para Documentação
- **Swagger Editor**: Visualizar a spec OpenAPI
- **Redoc**: Alternativa ao Swagger UI
- **Insomnia**: Gerar documentação a partir da coleção

## 🔗 Links Úteis

- [Swagger Editor](https://editor.swagger.io/) - Visualizar OpenAPI spec
- [Insomnia](https://insomnia.rest/) - Cliente REST
- [Postman](https://www.postman.com/) - Cliente REST alternativo
- [CrabCache GitHub](https://github.com/your-org/crabcache) - Código fonte

## 📝 Contribuindo

Para atualizar a documentação:

1. **API.md**: Documentação em markdown
2. **api-spec.yaml**: Especificação OpenAPI
3. **insomnia-collection.json**: Coleção do Insomnia

Mantenha os três arquivos sincronizados quando adicionar novos comandos ou funcionalidades.