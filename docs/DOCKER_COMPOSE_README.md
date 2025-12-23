# 🐳 CrabCache Docker Compose

## 🎯 Visão Geral

Este Docker Compose permite executar o CrabCache completo com HTTP Wrapper, testes de carga e monitoramento, tudo em containers isolados com limitação de recursos.

## 🚀 Início Rápido

### 1. Iniciar Serviços Básicos

```bash
# Iniciar CrabCache + HTTP Wrapper
./scripts/docker-start.sh

# Ou manualmente
docker-compose up -d
```

### 2. Verificar Status

```bash
# Status dos containers
docker-compose ps

# Logs em tempo real
docker-compose logs -f

# Health check
curl http://localhost:8000/health
```

### 3. Testar Funcionalidade

```bash
# PING
curl http://localhost:8000/ping

# PUT/GET
curl -X POST http://localhost:8000/put \
  -H "Content-Type: application/json" \
  -d '{"key": "test", "value": "docker-compose", "ttl": 3600}'

curl http://localhost:8000/get/test

# Estatísticas
curl http://localhost:8000/stats
```

## 🏗️ Arquitetura dos Serviços

### Serviços Principais

| Serviço | Porta | Descrição | Recursos |
|---------|-------|-----------|----------|
| `crabcache` | 7000 | Servidor TCP principal | 1 CPU, 512MB RAM |
| `http-wrapper` | 8000 | API HTTP para CrabCache | 0.5 CPU, 128MB RAM |

### Serviços Opcionais

| Serviço | Porta | Perfil | Descrição |
|---------|-------|--------|-----------|
| `load-tester` | - | `testing` | Testes de carga automatizados |
| `monitor` | 9100 | `monitoring` | Node Exporter para métricas |

## 📊 Perfis de Execução

### Perfil Padrão (Básico)
```bash
docker-compose up -d
```
- CrabCache + HTTP Wrapper apenas

### Perfil com Testes
```bash
docker-compose --profile testing up -d
```
- Inclui serviço de teste de carga

### Perfil com Monitoramento
```bash
docker-compose --profile monitoring up -d
```
- Inclui Node Exporter para métricas

### Todos os Perfis
```bash
docker-compose --profile testing --profile monitoring up -d
```

## 🔧 Scripts Disponíveis

### `./scripts/docker-start.sh`
Script principal para iniciar os serviços.

**Opções:**
- `--build` - Rebuild das imagens
- `--test` - Incluir testes de carga
- `--logs` - Mostrar logs em tempo real
- `--help` - Ajuda

**Exemplos:**
```bash
# Iniciar com rebuild
./scripts/docker-start.sh --build

# Iniciar com testes
./scripts/docker-start.sh --test

# Iniciar e ver logs
./scripts/docker-start.sh --logs
```

### `./scripts/resource-test.sh`
Testa diferentes perfis de recursos.

**Perfis:**
- `low` - Recursos muito limitados (0.5 CPU, 128MB)
- `medium` - Recursos moderados (1 CPU, 512MB) 
- `high` - Recursos abundantes (2 CPU, 1GB)

**Exemplos:**
```bash
# Teste com recursos baixos
./scripts/resource-test.sh low

# Teste com recursos altos
./scripts/resource-test.sh high
```

## 🎛️ Configuração de Recursos

### Limites Padrão

**CrabCache:**
- CPU: 1.0 core (reserva 0.5)
- Memória: 512MB (reserva 256MB)

**HTTP Wrapper:**
- CPU: 0.5 core (reserva 0.25)
- Memória: 128MB (reserva 64MB)

### Personalizar Recursos

Crie um `docker-compose.override.yml`:

```yaml
version: '3.8'

services:
  crabcache:
    deploy:
      resources:
        limits:
          cpus: '2.0'
          memory: 1G
        reservations:
          cpus: '1.0'
          memory: 512M

  http-wrapper:
    deploy:
      resources:
        limits:
          cpus: '1.0'
          memory: 256M
```

## 🧪 Testes de Carga

### Executar Teste Manual

```bash
# Iniciar com perfil de teste
docker-compose --profile testing up -d

# Executar teste
docker-compose exec load-tester python load_test.py

# Ver resultados
docker-compose exec load-tester cat /tmp/load_test_results.json
```

### Configurar Teste

Variáveis de ambiente no `docker-compose.yml`:

```yaml
load-tester:
  environment:
    - CONCURRENT_USERS=20      # Usuários simultâneos
    - TEST_DURATION=120        # Duração em segundos
    - CRABCACHE_HTTP_URL=http://http-wrapper:8000
```

### Métricas do Teste

O teste de carga mede:
- **Throughput**: Operações por segundo
- **Latência**: P50, P95, P99
- **Taxa de sucesso**: % de operações bem-sucedidas
- **Distribuição**: Por tipo de operação (PUT, GET, DEL, etc.)

## 📈 Monitoramento

### Logs dos Serviços

```bash
# Todos os logs
docker-compose logs -f

# Logs específicos
docker-compose logs -f crabcache
docker-compose logs -f http-wrapper

# Últimas 50 linhas
docker-compose logs --tail=50 crabcache
```

### Uso de Recursos

```bash
# Stats em tempo real
docker stats

# Stats específicos
docker stats crabcache-server crabcache-http-wrapper
```

### Health Checks

```bash
# Status dos health checks
docker-compose ps

# Testar manualmente
curl http://localhost:8000/health
echo "PING" | nc localhost 7000
```

## 🔍 Troubleshooting

### Serviços não iniciam

```bash
# Ver logs de erro
docker-compose logs

# Verificar recursos do sistema
docker system df
docker system prune  # Limpar se necessário
```

### Porta já em uso

```bash
# Verificar processos usando as portas
lsof -i :7000
lsof -i :8000

# Parar containers existentes
docker-compose down
docker stop $(docker ps -q)  # Parar todos se necessário
```

### Performance baixa

```bash
# Verificar limites de recursos
docker-compose config

# Aumentar recursos no override
cat > docker-compose.override.yml << EOF
version: '3.8'
services:
  crabcache:
    deploy:
      resources:
        limits:
          cpus: '2.0'
          memory: 1G
EOF
```

### Teste de carga falha

```bash
# Verificar conectividade
docker-compose exec load-tester curl http://http-wrapper:8000/health

# Ver logs do teste
docker-compose logs load-tester

# Reduzir carga
# Editar CONCURRENT_USERS e TEST_DURATION no docker-compose.yml
```

## 🎯 Cenários de Uso

### 1. Desenvolvimento Local
```bash
# Iniciar serviços básicos
./scripts/docker-start.sh

# Usar Insomnia com http://localhost:8000
# Desenvolver e testar funcionalidades
```

### 2. Teste de Performance
```bash
# Teste com recursos limitados
./scripts/resource-test.sh low

# Teste com carga alta
./scripts/resource-test.sh high --test
```

### 3. Demonstração
```bash
# Iniciar tudo com logs visíveis
./scripts/docker-start.sh --test --logs

# Mostrar métricas em tempo real
watch -n 2 'curl -s http://localhost:8000/stats | jq'
```

### 4. CI/CD Pipeline
```bash
# Build e teste automatizado
docker-compose build
docker-compose --profile testing up -d
docker-compose exec -T load-tester python load_test.py
docker-compose down
```

## 📚 Arquivos Relacionados

- `docker-compose.yml` - Configuração principal
- `Dockerfile` - CrabCache container
- `Dockerfile.wrapper` - HTTP Wrapper container  
- `Dockerfile.tester` - Load tester container
- `requirements-wrapper.txt` - Dependências Python
- `scripts/docker-start.sh` - Script de inicialização
- `scripts/resource-test.sh` - Testes de recursos
- `scripts/load_test.py` - Script de teste de carga

## 🎉 Próximos Passos

1. **Teste Básico**: Execute `./scripts/docker-start.sh`
2. **Importe Insomnia**: Use `docs/insomnia-collection.json`
3. **Teste Performance**: Execute `./scripts/resource-test.sh medium`
4. **Monitore**: Use `docker-compose logs -f` e `docker stats`
5. **Personalize**: Crie `docker-compose.override.yml` conforme necessário

---

**💡 Dica**: Use `docker-compose down -v` para limpar volumes e reiniciar do zero!