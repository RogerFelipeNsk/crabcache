# 📋 Guia Completo da Coleção Insomnia - CrabCache

## 🎯 Visão Geral

Esta é a coleção **COMPLETA** do Insomnia para testar todas as funcionalidades do CrabCache via HTTP Wrapper. Inclui todos os endpoints, cenários de teste e exemplos práticos.

## 📦 Importar Coleção

### Arquivo da Coleção
- **Arquivo**: `docs/insomnia-collection-complete.json`
- **Versão**: Completa com todos os endpoints
- **Total de Requisições**: 25+ requisições organizadas

### Como Importar
1. Abra o Insomnia
2. Clique em **"Import/Export"** → **"Import Data"**
3. Selecione o arquivo: `docs/insomnia-collection-complete.json`
4. Escolha o ambiente: **"Local Development"** ou **"Docker Compose"**

## 🏗️ Estrutura da Coleção

### 📋 1. Informações e Documentação
- **📖 Documentação da API** - `GET /`
  - Retorna todos os endpoints disponíveis
  - Exemplos de uso para cada endpoint

### 🏥 2. Health Check e Status
- **🩺 Health Check Completo** - `GET /health`
  - Status do wrapper + conectividade CrabCache
- **🏓 PING - Teste de Conectividade** - `GET /ping`
  - Teste básico de conectividade

### 🔧 3. Operações Básicas
- **💾 PUT - Armazenar Valor Simples** - `POST /put`
- **⏰ PUT - Armazenar com TTL** - `POST /put`
- **📄 PUT - Armazenar JSON** - `POST /put`
- **📥 GET - Recuperar Valor Simples** - `GET /get/{key}`
- **🔐 GET - Recuperar Sessão** - `GET /get/{key}`
- **📋 GET - Recuperar JSON** - `GET /get/{key}`
- **❌ GET - Chave Inexistente** - `GET /get/{key}`
- **🗑️ DELETE - Remover Chave** - `DELETE /delete/{key}`
- **❌ DELETE - Chave Inexistente** - `DELETE /delete/{key}`

### ⏰ 4. Gerenciamento TTL
- **⏰ EXPIRE - TTL 1 Hora** - `POST /expire`
- **⏱️ EXPIRE - TTL 5 Minutos** - `POST /expire`
- **📅 EXPIRE - TTL 1 Dia** - `POST /expire`
- **❌ EXPIRE - Chave Inexistente** - `POST /expire`

### 📊 5. Monitoramento e Estatísticas
- **📈 STATS - Estatísticas Detalhadas** - `GET /stats`

### 🔧 6. Comandos Raw
- **🏓 RAW - PING** - `POST /command`
- **💾 RAW - PUT** - `POST /command`
- **📥 RAW - GET** - `POST /command`
- **🗑️ RAW - DEL** - `POST /command`
- **⏰ RAW - EXPIRE** - `POST /command`
- **📊 RAW - STATS** - `POST /command`

### 🎯 7. Cenários de Teste
- **🔐 Cenário - Criar Sessão de Usuário**
- **🌐 Cenário - Cache de API Externa**
- **⚙️ Cenário - Cache de Configuração**

## 🌍 Ambientes Configurados

### Base Environment
```json
{
  "base_url": "http://localhost:8000",
  "crabcache_tcp": "localhost:7001",
  "test_key": "test:insomnia",
  "test_value": "valor_de_teste",
  "test_ttl": 3600
}
```

### Local Development
- **URL**: `http://localhost:8000`
- **Uso**: Desenvolvimento local com HTTP wrapper

### Docker Compose
- **URL**: `http://localhost:8000`
- **Uso**: Ambiente Docker Compose

## 🚀 Fluxo de Teste Recomendado

### 1. Verificação Inicial (2 min)
```
1. 🩺 Health Check Completo
2. 🏓 PING - Teste de Conectividade
3. 📖 Documentação da API
```

### 2. Operações Básicas (5 min)
```
1. 💾 PUT - Armazenar Valor Simples
2. 📥 GET - Recuperar Valor Simples
3. 🗑️ DELETE - Remover Chave
4. ❌ GET - Chave Inexistente (deve retornar NULL)
```

### 3. Teste com TTL (3 min)
```
1. ⏰ PUT - Armazenar com TTL
2. 🔐 GET - Recuperar Sessão
3. ⏱️ EXPIRE - TTL 5 Minutos
4. 🔐 GET - Recuperar Sessão (novamente)
```

### 4. Dados Complexos (3 min)
```
1. 📄 PUT - Armazenar JSON
2. 📋 GET - Recuperar JSON
3. Verificar se JSON foi preservado
```

### 5. Monitoramento (2 min)
```
1. 📈 STATS - Estatísticas Detalhadas
2. Analisar distribuição de chaves
3. Verificar uso de memória
```

### 6. Comandos Raw (5 min)
```
1. 🏓 RAW - PING
2. 💾 RAW - PUT
3. 📥 RAW - GET
4. 🗑️ RAW - DEL
5. 📊 RAW - STATS
```

### 7. Cenários Reais (10 min)
```
1. 🔐 Cenário - Criar Sessão de Usuário
2. 🌐 Cenário - Cache de API Externa
3. ⚙️ Cenário - Cache de Configuração
4. Testar recuperação de cada cenário
```

## 📝 Exemplos de Uso

### PUT com JSON
```json
{
  "key": "user:profile:123",
  "value": "{\"name\":\"João Silva\",\"age\":30,\"email\":\"joao@example.com\",\"active\":true}",
  "ttl": 7200
}
```

### EXPIRE
```json
{
  "key": "session:user_123",
  "ttl": 1800
}
```

### Comando Raw
```json
{
  "command": "PUT raw:test valor_raw_test 1800"
}
```

## 🔍 Respostas Esperadas

### Sucesso
```json
{
  "command": "PUT user:123 john_doe 3600",
  "response": "OK",
  "success": true
}
```

### Chave Não Encontrada
```json
{
  "command": "GET chave:inexistente",
  "response": "NULL",
  "success": false,
  "value": null
}
```

### Estatísticas
```json
{
  "command": "STATS",
  "parsed": {
    "shard_0": "10 keys",
    "total": "10 keys"
  },
  "response": "STATS: shard_0: 10 keys, 1024B/1073741824B, total: 10 keys, 1024B/1073741824B memory",
  "success": true
}
```

### Health Check
```json
{
  "crabcache": "PONG",
  "healthy": true,
  "wrapper": "OK"
}
```

## 🎛️ Variáveis de Ambiente

### Personalizáveis
- `test_key` - Chave para testes (padrão: "test:insomnia")
- `test_value` - Valor para testes (padrão: "valor_de_teste")
- `test_ttl` - TTL para testes (padrão: 3600)

### Como Usar
1. Vá em **Environments** no Insomnia
2. Edite o ambiente ativo
3. Modifique as variáveis conforme necessário
4. As requisições usarão automaticamente as novas variáveis

## 🐛 Troubleshooting

### Erro: Connection Refused
```json
{
  "crabcache": "ERROR: [Errno 111] Connection refused",
  "healthy": false,
  "wrapper": "OK"
}
```
**Solução**: Verifique se o CrabCache está rodando
```bash
./scripts/docker-start.sh
```

### Erro: Method Not Allowed
```
Method Not Allowed
The method is not allowed for the requested URL.
```
**Solução**: Você está usando endpoint incorreto. Use os endpoints da coleção.

### Erro: Invalid Command
```json
{
  "command": "PUT test valor com espaços",
  "response": "ERROR: Invalid command",
  "success": false
}
```
**Solução**: Valores com espaços precisam ser tratados. Use underscore ou JSON.

### Wrapper Não Responde
**Sintomas**: Timeout nas requisições
**Solução**:
```bash
# Verificar status
curl http://localhost:8000/health

# Reiniciar se necessário
docker-compose restart http-wrapper
```

## 📊 Métricas de Performance

### Latência Esperada
- **PING**: < 5ms
- **PUT/GET/DEL**: < 10ms
- **STATS**: < 15ms

### Taxa de Sucesso
- **Operações básicas**: > 95%
- **Health checks**: 100%
- **Comandos raw**: > 95%

## 🎯 Casos de Uso Avançados

### 1. Cache de Sessão Web
```json
{
  "key": "session:web_abc123",
  "value": "{\"user_id\":456,\"role\":\"user\",\"permissions\":[\"read\",\"write\"]}",
  "ttl": 1800
}
```

### 2. Cache de Resultado de Query
```json
{
  "key": "query:users:active",
  "value": "{\"count\":1250,\"last_updated\":\"2025-12-21T00:00:00Z\"}",
  "ttl": 300
}
```

### 3. Cache de Configuração Dinâmica
```json
{
  "key": "config:feature_flags",
  "value": "{\"new_ui\":true,\"beta_features\":false,\"maintenance_mode\":false}",
  "ttl": 86400
}
```

## 📚 Recursos Adicionais

- **Documentação Completa**: `docs/API.md`
- **Guia Docker Compose**: `DOCKER_COMPOSE_README.md`
- **Especificação OpenAPI**: `docs/api-spec.yaml`
- **Testes Python**: `docs/test_api.py`

## 🎉 Próximos Passos

1. **Importe a coleção**: `insomnia-collection-complete.json`
2. **Execute o fluxo básico**: Health → PUT → GET → DELETE
3. **Teste cenários reais**: Sessões, APIs, configurações
4. **Monitore performance**: Use STATS regularmente
5. **Experimente comandos raw**: Para casos avançados

---

**💡 Dica**: Use a aba "Timeline" do Insomnia para ver o histórico de todas as requisições e analisar padrões de uso!