# 🚀 Referência Rápida - Endpoints CrabCache HTTP Wrapper

## ❌ ERRO COMUM - O que NÃO fazer:

```bash
# ❌ INCORRETO - Não funciona!
curl --request POST \
--url http://localhost:8000/ \
--header 'Content-Type: text/plain' \
--data 'PING'
```

**Erro**: `Method Not Allowed - The method is not allowed for the requested URL.`

## ✅ FORMAS CORRETAS:

### 1. 🏓 PING - Teste de Conectividade
```bash
# Método 1: Endpoint específico (RECOMENDADO)
curl http://localhost:8000/ping

# Método 2: Comando raw
curl -X POST http://localhost:8000/command \
  -H "Content-Type: application/json" \
  -d '{"command": "PING"}'
```

### 2. 🩺 Health Check Completo
```bash
curl http://localhost:8000/health
```

### 3. 📖 Ver Documentação
```bash
curl http://localhost:8000/
```

### 4. 💾 PUT - Armazenar Dados
```bash
# Valor simples
curl -X POST http://localhost:8000/put \
  -H "Content-Type: application/json" \
  -d '{"key": "usuario:123", "value": "joao_silva"}'

# Com TTL (1 hora)
curl -X POST http://localhost:8000/put \
  -H "Content-Type: application/json" \
  -d '{"key": "sessao:abc", "value": "dados_sessao", "ttl": 3600}'
```

### 5. 📥 GET - Recuperar Dados
```bash
curl http://localhost:8000/get/usuario:123
curl http://localhost:8000/get/sessao:abc
```

### 6. 🗑️ DELETE - Remover Dados
```bash
curl -X DELETE http://localhost:8000/delete/usuario:123
```

### 7. ⏰ EXPIRE - Definir TTL
```bash
curl -X POST http://localhost:8000/expire \
  -H "Content-Type: application/json" \
  -d '{"key": "usuario:123", "ttl": 1800}'
```

### 8. 📊 STATS - Estatísticas
```bash
curl http://localhost:8000/stats
```

### 9. 🔧 Comando Raw (Avançado)
```bash
# Qualquer comando TCP
curl -X POST http://localhost:8000/command \
  -H "Content-Type: application/json" \
  -d '{"command": "PUT raw:test valor_teste 3600"}'
```

## 📋 Todos os Endpoints Disponíveis

| Método | Endpoint | Descrição |
|--------|----------|-----------|
| `GET` | `/` | Documentação da API |
| `GET` | `/health` | Health check completo |
| `GET` | `/ping` | PING do CrabCache |
| `POST` | `/put` | Armazenar chave-valor |
| `GET` | `/get/<key>` | Recuperar valor |
| `DELETE` | `/delete/<key>` | Remover chave |
| `POST` | `/expire` | Definir TTL |
| `GET` | `/stats` | Estatísticas do servidor |
| `POST` | `/command` | Comando TCP raw |

## 🎯 Exemplos Práticos

### Cenário: Cache de Sessão Web
```bash
# 1. Criar sessão
curl -X POST http://localhost:8000/put \
  -H "Content-Type: application/json" \
  -d '{"key": "sessao:user123", "value": "dados_da_sessao", "ttl": 1800}'

# 2. Verificar sessão
curl http://localhost:8000/get/sessao:user123

# 3. Renovar sessão (mais 30 min)
curl -X POST http://localhost:8000/expire \
  -H "Content-Type: application/json" \
  -d '{"key": "sessao:user123", "ttl": 1800}'

# 4. Logout (remover sessão)
curl -X DELETE http://localhost:8000/delete/sessao:user123
```

### Cenário: Cache de API Externa
```bash
# 1. Cachear resposta de API (10 minutos)
curl -X POST http://localhost:8000/put \
  -H "Content-Type: application/json" \
  -d '{"key": "api:clima:saopaulo", "value": "{\"temp\":25,\"condicao\":\"ensolarado\"}", "ttl": 600}'

# 2. Recuperar dados cacheados
curl http://localhost:8000/get/api:clima:saopaulo
```

## 🔍 Respostas Esperadas

### ✅ Sucesso
```json
{
  "command": "PING",
  "response": "PONG",
  "success": true
}
```

### ❌ Chave Não Encontrada
```json
{
  "command": "GET chave:inexistente",
  "response": "NULL",
  "success": false,
  "value": null
}
```

### 🩺 Health Check
```json
{
  "crabcache": "PONG",
  "healthy": true,
  "wrapper": "OK"
}
```

## 🚨 Problemas Comuns

### 1. "Method Not Allowed"
- **Causa**: Usando endpoint incorreto ou método HTTP errado
- **Solução**: Use os endpoints desta referência

### 2. "Connection Refused"
- **Causa**: CrabCache não está rodando
- **Solução**: `./scripts/docker-start.sh`

### 3. "Invalid Command"
- **Causa**: Valores com espaços em comandos raw
- **Solução**: Use endpoints específicos ou escape espaços

## 🎉 Teste Rápido (30 segundos)

```bash
# 1. Verificar se está funcionando
curl http://localhost:8000/health

# 2. Teste básico
curl http://localhost:8000/ping

# 3. Armazenar algo
curl -X POST http://localhost:8000/put \
  -H "Content-Type: application/json" \
  -d '{"key": "teste", "value": "funcionando"}'

# 4. Recuperar
curl http://localhost:8000/get/teste

# 5. Ver estatísticas
curl http://localhost:8000/stats
```

---

**💡 Dica**: Use a coleção completa do Insomnia (`insomnia-collection-complete.json`) para ter todos esses exemplos prontos para usar!