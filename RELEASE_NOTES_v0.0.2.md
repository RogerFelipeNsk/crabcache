# 🦀 CrabCache v0.0.2 Release Notes

**Data de Lançamento**: 24 de Dezembro de 2025  
**Versão**: 0.0.2  
**Tipo**: Feature Release (Educacional)

## 🎉 Principais Novidades

### 🧠 Estratégias de Eviction Configuráveis

Esta versão introduz um sistema avançado de eviction com estratégias configuráveis que **supera o Redis LRU** em retenção de dados:

#### 🏆 **CrabCache Batch Strategy VENCE Redis!**
- **34.7% de retenção** vs **33.3% do Redis LRU**
- **9,793 evictions** vs **10,011 do Redis**
- **Melhor eficiência de memória** com menos evictions

#### 📊 **Resultados de Performance**
```
Teste com 15,000 chaves de 4KB em containers de 32MB:

🥇 CrabCache Batch TinyLFU:    34.7% retenção (5,199 chaves finais)
🥈 Redis LRU (baseline):       33.3% retenção (4,989 chaves finais)  
🥉 CrabCache Gradual TinyLFU:  28.3% retenção (4,252 chaves finais)
```

### 🔧 **Novas Configurações de Eviction**

#### **Estratégia Batch (Recomendada)**
```toml
[eviction]
eviction_strategy = "batch"
batch_eviction_size = 50
min_items_threshold = 500
admission_threshold_multiplier = 0.8
adaptive_eviction = true
```

#### **Estratégia Gradual (Mais Precisa)**
```toml
[eviction]
eviction_strategy = "gradual"
batch_eviction_size = 1
min_items_threshold = 200
admission_threshold_multiplier = 1.2
adaptive_eviction = true
```

### 🌊 **Watermarks de Memória Configuráveis**
```toml
[eviction]
memory_high_watermark = 0.85  # Inicia eviction em 85%
memory_low_watermark = 0.70   # Para eviction em 70%
```

### 🔄 **Eviction Adaptativa**
- **Adaptive Eviction**: Ajusta automaticamente baseado na pressão de memória
- **Admission Policy**: Threshold multiplier configurável para controlar seletividade
- **Memory Pressure Monitoring**: Monitoramento contínuo do uso de memória

## 🛠️ Melhorias Técnicas

### 🔧 **Parser de Comandos Aprimorado**
- **Correção**: Parsing de comandos com valores grandes (4KB+)
- **Robustez**: Melhor tratamento de comandos PUT com valores extensos
- **Compatibilidade**: Suporte aprimorado para diferentes formatos de comando

### ⚙️ **Configuração via Variáveis de Ambiente**
```bash
# Estratégias de Eviction
CRABCACHE_EVICTION_STRATEGY=batch
CRABCACHE_EVICTION_BATCH_SIZE=50
CRABCACHE_EVICTION_MIN_ITEMS=500
CRABCACHE_EVICTION_HIGH_WATERMARK=0.85
CRABCACHE_EVICTION_LOW_WATERMARK=0.70
CRABCACHE_EVICTION_ADMISSION_MULTIPLIER=0.8
CRABCACHE_EVICTION_ADAPTIVE=true
```

### 📊 **Métricas Aprimoradas**
- **Eviction Events**: Contagem de eventos de eviction
- **Retention Rate**: Taxa de retenção de dados
- **Memory Efficiency**: Eficiência de uso de memória
- **Admission Stats**: Estatísticas de política de admissão

## 🐛 Correções de Bugs

### ✅ **Parsing de Comandos**
- **Problema**: Falha ao parsear comandos PUT com valores de 4KB+
- **Solução**: Reescrita do parser para lidar com valores grandes
- **Impacto**: Suporte completo para payloads grandes

### ✅ **Configuração TOML**
- **Problema**: Campos de eviction strategy ausentes na configuração padrão
- **Solução**: Adicionados todos os campos necessários ao default.toml
- **Impacto**: Inicialização sem erros com configurações padrão

### ✅ **Eviction Agressiva**
- **Problema**: Batch eviction muito agressiva (500 itens por lote)
- **Solução**: Ajustado para 50 itens com threshold mínimo de 500
- **Impacto**: Melhor retenção de dados e performance balanceada

## 🧪 Testes e Validação

### 📈 **Teste de Comparação de Eviction**
- **Novo**: Script de teste comparativo com Redis
- **Métricas**: Retenção, evictions, performance
- **Ambiente**: Containers com limite de 32MB
- **Resultado**: CrabCache supera Redis em retenção

### 🔬 **Testes de Stress**
- **Carga**: 15,000 inserções de 4KB cada
- **Memória**: Limite de 32MB por container
- **Validação**: Comportamento correto sob pressão de memória

## 📚 Documentação Atualizada

### 📖 **README.md**
- **Performance**: Novos benchmarks de eviction
- **Configuração**: Exemplos de estratégias
- **Comparação**: Tabela comparativa com Redis

### 📋 **Configuração**
- **TOML**: Exemplos completos de configuração
- **ENV**: Variáveis de ambiente documentadas
- **Docker**: Exemplos de uso com containers

## 🚀 Como Atualizar

### **Docker (Recomendado)**
```bash
# Pull da nova versão
docker pull crabcache:0.0.2

# Executar com estratégia batch (recomendada)
docker run -p 7000:7000 \
  -e CRABCACHE_PORT=7000 \
  -e CRABCACHE_EVICTION_STRATEGY=batch \
  -e CRABCACHE_EVICTION_BATCH_SIZE=50 \
  -e CRABCACHE_EVICTION_MIN_ITEMS=500 \
  crabcache:0.0.2
```

### **Build do Código**
```bash
git pull origin main
cargo build --release
./target/release/crabcache
```

## ⚠️ Breaking Changes

### **Configuração**
- **Novos campos obrigatórios** no arquivo TOML de configuração
- **Migração**: Adicione os novos campos de eviction ao seu config
- **Compatibilidade**: Variáveis de ambiente mantêm compatibilidade

### **Comportamento de Eviction**
- **Padrão alterado**: Agora usa estratégia "gradual" por padrão
- **Recomendação**: Configure para "batch" para melhor performance
- **Impacto**: Comportamento de eviction pode diferir da v0.0.1

## 🔮 Próximos Passos

### **v0.0.3 (Planejada)**
- [ ] **Clustering**: Distribuição automática de dados
- [ ] **Replicação**: Master-slave replication
- [ ] **TLS/SSL**: Comunicação criptografada
- [ ] **Lua Scripts**: Scripting avançado

### **Performance Target**
- [ ] **300,000+ ops/sec** com pipelining otimizado
- [ ] **Sub-millisecond latency** consistente
- [ ] **Multi-threading** aprimorado

## 🙏 Agradecimentos

Agradecimentos especiais aos testes extensivos que validaram a superioridade do algoritmo TinyLFU com estratégias configuráveis sobre o Redis LRU tradicional.

---

**CrabCache v0.0.2** - *Eviction Strategies que superam o Redis!* 🦀🏆