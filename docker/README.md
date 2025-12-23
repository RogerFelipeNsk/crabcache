# Docker - CrabCache Containerization

Esta pasta contém todos os arquivos relacionados ao Docker e containerização do CrabCache.

## 📁 Estrutura

```
docker/
├── README.md                    # Este arquivo
├── Dockerfile                   # Container principal do CrabCache
├── Dockerfile.tester           # Container para testes
├── Dockerfile.wrapper          # Container HTTP wrapper
├── requirements-wrapper.txt    # Dependências Python do wrapper
└── compose/
    ├── docker-compose.yml      # Orquestração principal
    └── docker-compose.redis.yml # Comparação com Redis
```

## 🚀 Como Usar

### 1. Build da Imagem Principal
```bash
cd crabcache
docker build -f docker/Dockerfile -t crabcache:latest .
```

### 2. Build do HTTP Wrapper
```bash
docker build -f docker/Dockerfile.wrapper -t crabcache-wrapper:latest .
```

### 3. Build do Container de Testes
```bash
docker build -f docker/Dockerfile.tester -t crabcache-tester:latest .
```

### 4. Executar com Docker Compose
```bash
# Orquestração completa
docker-compose -f docker/compose/docker-compose.yml up

# Comparação com Redis
docker-compose -f docker/compose/docker-compose.redis.yml up
```

## 🐳 Containers Disponíveis

### CrabCache Principal
- **Imagem**: `crabcache:latest`
- **Porta**: 7001 (TCP)
- **Porta**: 9090 (Métricas HTTP)
- **Uso**: Cache principal com observabilidade

### HTTP Wrapper
- **Imagem**: `crabcache-wrapper:latest`
- **Porta**: 8000 (HTTP)
- **Uso**: Interface HTTP para testes com Insomnia/Postman

### Container de Testes
- **Imagem**: `crabcache-tester:latest`
- **Uso**: Execução de benchmarks e testes automatizados

## 📊 Monitoramento

### Métricas Prometheus
- **URL**: http://localhost:9090/metrics
- **Dashboard**: http://localhost:9090/dashboard
- **Health**: http://localhost:9090/health

### Logs
```bash
# Ver logs do CrabCache
docker-compose -f docker/compose/docker-compose.yml logs crabcache

# Ver logs do wrapper
docker-compose -f docker/compose/docker-compose.yml logs wrapper
```

## 🔧 Configuração

### Variáveis de Ambiente
- `CRABCACHE_PORT`: Porta TCP (padrão: 7001)
- `CRABCACHE_METRICS_PORT`: Porta métricas (padrão: 9090)
- `CRABCACHE_BIND_ADDR`: Endereço bind (padrão: 0.0.0.0)
- `CRABCACHE_NUM_SHARDS`: Número de shards (padrão: 4)

### Volumes
- `/app/data`: Dados persistentes (se WAL habilitado)
- `/app/config`: Arquivos de configuração

## 🧪 Testes

### Teste Rápido
```bash
# Testar conectividade
echo "PING" | nc localhost 7001

# Testar HTTP wrapper
curl http://localhost:8000/ping
```

### Benchmarks
```bash
# Executar container de testes
docker run --rm --network host crabcache-tester:latest

# Ou usar scripts específicos
docker-compose -f docker/compose/docker-compose.yml exec tester python3 /app/scripts/test_observability.py
```

## 📈 Performance

### Configuração Recomendada
- **CPU**: 4+ cores
- **RAM**: 2GB+ disponível
- **Rede**: Baixa latência para melhor performance

### Limites de Recursos
```yaml
# No docker-compose.yml
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
```

## 🔗 Documentação Relacionada

- `../docs/DOCKER_COMPOSE_README.md` - Guia detalhado do Docker Compose
- `../docs/HTTP_WRAPPER_README.md` - Documentação do HTTP Wrapper
- `../scripts/test_docker.py` - Scripts de teste Docker
- `../scripts/test_docker_simple.sh` - Testes simples

---

**CrabCache containerizado e pronto para produção!** 🐳