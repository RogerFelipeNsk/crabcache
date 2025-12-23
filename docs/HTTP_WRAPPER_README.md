# 🌐 CrabCache HTTP Wrapper

## ✅ Status: FUNCIONANDO

O HTTP Wrapper está **totalmente funcional** e pronto para uso com Insomnia!

## 🚀 Como Usar

### 1. Iniciar os Serviços

O CrabCache já está rodando no Docker na porta 7005. Para iniciar o HTTP Wrapper:

```bash
# Método 1: Usar o script automático
./scripts/start-wrapper.sh docker

# Método 2: Iniciar manualmente
cd crabcache
CRABCACHE_PORT=7005 python3 http_wrapper.py
```

### 2. Testar Funcionamento

```bash
# Verificar saúde
curl http://localhost:8000/health

# Testar PING
curl http://localhost:8000/ping

# Armazenar valor
curl -X POST http://localhost:8000/put \
  -H "Content-Type: application/json" \
  -d '{"key": "usuario:123", "value": "João Silva", "ttl": 3600}'

# Recuperar valor
curl http://localhost:8000/get/usuario:123

# Ver estatísticas
curl http://localhost:8000/stats
```

### 3. Usar no Insomnia

1. **Importar Coleção**: `docs/insomnia-collection.json`
2. **Selecionar Ambiente**: "Local Development" 
3. **Base URL**: `http://localhost:8000` (já configurado)
4. **Executar Requisições**: Todas as requisições estão prontas!

## 📋 Endpoints Disponíveis

| Método | Endpoint | Descrição |
|--------|----------|-----------|
| GET | `/health` | Status do wrapper + CrabCache |
| GET | `/ping` | PING do CrabCache |
| POST | `/put` | Armazenar chave-valor |
| GET | `/get/<key>` | Recuperar valor |
| DELETE | `/delete/<key>` | Remover chave |
| POST | `/expire` | Definir TTL |
| GET | `/stats` | Estatísticas do servidor |
| POST | `/command` | Comando TCP raw |

## 🎯 Teste Rápido no Insomnia

1. **Health Check**: Execute "HEALTH - Status do Wrapper"
2. **PING**: Execute "PING - Health Check"  
3. **PUT**: Execute "PUT - Armazenar Valor Simples"
4. **GET**: Execute "GET - Recuperar Usuário"
5. **STATS**: Execute "STATS - Estatísticas do Servidor"

## ✅ Testes Realizados

- [x] HTTP Wrapper iniciando corretamente
- [x] Conexão com CrabCache Docker (porta 7005)
- [x] Endpoint `/health` funcionando
- [x] Endpoint `/ping` funcionando  
- [x] Operação PUT funcionando
- [x] Operação GET funcionando
- [x] Operação DELETE funcionando
- [x] Endpoint `/stats` funcionando
- [x] Parsing de respostas JSON
- [x] Coleção Insomnia atualizada para HTTP

## 🔧 Configuração Atual

- **CrabCache**: Docker container na porta 7005
- **HTTP Wrapper**: Porta 8000
- **Coleção Insomnia**: Configurada para `http://localhost:8000`
- **Flask**: Instalado e funcionando

## 📚 Documentação

- **Guia Completo**: `docs/INSOMNIA_GUIDE.md`
- **API Docs**: `docs/API.md`
- **Coleção**: `docs/insomnia-collection.json`

## 🎉 Resultado

**O HTTP Wrapper está 100% funcional!** 

Você pode agora:
1. ✅ Usar o Insomnia normalmente com a coleção importada
2. ✅ Fazer todas as operações do CrabCache via HTTP
3. ✅ Testar TTL, sharding, e todas as funcionalidades
4. ✅ Monitorar estatísticas em tempo real

**Próximo passo**: Importe a coleção no Insomnia e comece a testar! 🚀