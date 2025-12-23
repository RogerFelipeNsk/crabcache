# Guia de Uso - CrabCache com Insomnia

Este guia explica como usar o CrabCache com o Insomnia através do HTTP Wrapper.

## 🎯 Visão Geral

O CrabCache usa protocolo TCP nativo, mas para facilitar testes com ferramentas HTTP como Insomnia, criamos um HTTP Wrapper que converte chamadas HTTP em comandos TCP.

**Arquitetura:**
```
Insomnia → HTTP Wrapper (porta 8000) → CrabCache TCP (porta 7000/7004)
```

## 🚀 Início Rápido

### 1. Iniciar os Serviços

**Opção A: Local Development**
```bash
./scripts/start-wrapper.sh local
```

**Opção B: Docker Container**
```bash
./scripts/start-wrapper.sh docker
```

### 2. Verificar se está funcionando

```bash
# Testar HTTP Wrapper
curl http://localhost:8000/health

# Testar CrabCache via wrapper
curl http://localhost:8000/ping
```

### 3. Importar Coleção no Insomnia

1. Abra o Insomnia
2. Clique em "Import/Export" → "Import Data"
3. Selecione o arquivo: `docs/insomnia-collection.json`
4. Escolha o ambiente "Local Development"

## 📋 Coleção do Insomnia

A coleção inclui os seguintes grupos de requisições:

### 🏥 Health Check
- **PING** - Verifica se o CrabCache está respondendo

### 🔧 Operações Básicas
- **PUT - Armazenar Valor Simples** - Armazena chave-valor
- **PUT - Armazenar com TTL** - Armazena com expiração
- **PUT - Armazenar JSON** - Armazena dados JSON
- **GET - Recuperar Usuário** - Recupera valor por chave
- **GET - Recuperar Sessão** - Testa recuperação com TTL
- **GET - Recuperar JSON** - Recupera dados JSON
- **DEL - Remover Usuário** - Remove chave do cache

### ⏰ Gerenciamento TTL
- **EXPIRE - TTL 1 Hora** - Define TTL de 3600 segundos
- **EXPIRE - TTL 5 Minutos** - Define TTL de 300 segundos
- **EXPIRE - TTL 1 Dia** - Define TTL de 86400 segundos

### 📊 Monitoramento
- **STATS** - Estatísticas detalhadas dos shards
- **RAW Command** - Enviar comando TCP personalizado
- **HEALTH** - Status do HTTP wrapper

## 🌐 Endpoints HTTP

### GET /health
Verifica status do wrapper e conexão com CrabCache.

**Resposta:**
```json
{
  "wrapper": "OK",
  "crabcache": "PONG",
  "healthy": true
}
```

### GET /ping
Executa comando PING no CrabCache.

**Resposta:**
```json
{
  "command": "PING",
  "response": "PONG",
  "success": true
}
```

### POST /put
Armazena chave-valor no cache.

**Body:**
```json
{
  "key": "user:123",
  "value": "john_doe",
  "ttl": 3600  // opcional
}
```

**Resposta:**
```json
{
  "command": "PUT user:123 john_doe 3600",
  "response": "OK",
  "success": true
}
```

### GET /get/{key}
Recupera valor de uma chave.

**Resposta:**
```json
{
  "command": "GET user:123",
  "response": "john_doe",
  "success": true,
  "value": "john_doe"
}
```

### DELETE /delete/{key}
Remove uma chave do cache.

**Resposta:**
```json
{
  "command": "DEL user:123",
  "response": "OK",
  "success": true
}
```

### POST /expire
Define TTL para uma chave existente.

**Body:**
```json
{
  "key": "user:123",
  "ttl": 1800
}
```

**Resposta:**
```json
{
  "command": "EXPIRE user:123 1800",
  "response": "OK",
  "success": true
}
```

### GET /stats
Obtém estatísticas do servidor.

**Resposta:**
```json
{
  "command": "STATS",
  "response": "STATS: shard_0: 10 keys, 1024B/1073741824B, total: 10 keys, 1024B/1073741824B memory",
  "success": true,
  "parsed": {
    "shard_0": "10 keys, 1024B/1073741824B",
    "total": "10 keys, 1024B/1073741824B memory"
  }
}
```

### POST /command
Envia comando TCP raw.

**Body:**
```json
{
  "command": "PING"
}
```

**Resposta:**
```json
{
  "command": "PING",
  "response": "PONG"
}
```

## 🔄 Fluxo de Teste Recomendado

### 1. Verificação Inicial
1. Execute "HEALTH - Status do Wrapper"
2. Execute "PING - Health Check"

### 2. Operações Básicas
1. Execute "PUT - Armazenar Valor Simples"
2. Execute "GET - Recuperar Usuário"
3. Execute "DEL - Remover Usuário"
4. Execute novamente "GET - Recuperar Usuário" (deve retornar NULL)

### 3. Teste com TTL
1. Execute "PUT - Armazenar com TTL"
2. Execute "GET - Recuperar Sessão" (deve retornar o valor)
3. Execute "EXPIRE - TTL 5 Minutos" para alterar TTL
4. Aguarde alguns segundos e execute "GET - Recuperar Sessão" novamente

### 4. Dados JSON
1. Execute "PUT - Armazenar JSON"
2. Execute "GET - Recuperar JSON"
3. Verifique se o JSON foi preservado corretamente

### 5. Monitoramento
1. Execute "STATS - Estatísticas do Servidor"
2. Analise a distribuição de chaves entre shards
3. Monitore uso de memória

## 🐛 Troubleshooting

### Erro de Conexão
```json
{
  "command": "PING",
  "response": "ERROR: [Errno 61] Connection refused"
}
```
**Solução:** Verifique se o CrabCache está rodando na porta correta.

### Wrapper não responde
**Sintomas:** Timeout nas requisições HTTP
**Solução:** 
1. Verifique se o wrapper está rodando: `curl http://localhost:8000/health`
2. Reinicie com: `./scripts/start-wrapper.sh local`

### Porta em uso
```
❌ Porta 8000 já está em uso
```
**Solução:**
1. Encontre o processo: `lsof -i :8000`
2. Mate o processo: `kill -9 <PID>`
3. Ou use porta diferente editando `http_wrapper.py`

### CrabCache não responde
```json
{
  "wrapper": "OK",
  "crabcache": "ERROR: Connection refused",
  "healthy": false
}
```
**Solução:**
1. Verifique se CrabCache está rodando: `ps aux | grep crabcache`
2. Verifique logs: `docker logs <container_id>` (se usando Docker)
3. Reinicie o CrabCache

## 📝 Personalização

### Alterar Portas
Edite as variáveis no início do `http_wrapper.py`:
```python
CRABCACHE_HOST = 'localhost'
CRABCACHE_PORT = 7000  # ou 7004 para Docker
```

### Adicionar Novos Endpoints
1. Adicione nova rota no `http_wrapper.py`
2. Implemente a lógica de conversão HTTP → TCP
3. Adicione nova requisição na coleção do Insomnia

### Usar com Docker Compose
Crie um `docker-compose.yml`:
```yaml
version: '3.8'
services:
  crabcache:
    build: .
    ports:
      - "7000:7000"
  
  wrapper:
    build:
      context: .
      dockerfile: Dockerfile.wrapper
    ports:
      - "8000:8000"
    depends_on:
      - crabcache
    environment:
      - CRABCACHE_HOST=crabcache
      - CRABCACHE_PORT=7000
```

## 🎯 Próximos Passos

1. **Teste Performance**: Use a coleção para testar diferentes cenários
2. **Monitore Métricas**: Use o endpoint `/stats` regularmente
3. **Teste TTL**: Experimente diferentes valores de TTL
4. **Teste Sharding**: Armazene muitas chaves e veja a distribuição
5. **Integre na Aplicação**: Use os exemplos para integrar em seu código

## 📚 Recursos Adicionais

- **Documentação Completa**: `docs/API.md`
- **Especificação OpenAPI**: `docs/api-spec.yaml`
- **Testes Python**: `docs/test_api.py`
- **Cliente Exemplo**: `examples/simple_client.rs`

---

**💡 Dica:** Mantenha o terminal com os logs abertos para monitorar as operações em tempo real!