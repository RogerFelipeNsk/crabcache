# Configuração do Deploy Automático Docker

Este documento explica como configurar o deploy automático do Docker Hub quando há push na branch `main`.

## Secrets Necessárias

Para que o deploy automático funcione, você precisa configurar as seguintes secrets no GitHub:

### 1. DOCKER_USERNAME
- **Valor**: Seu nome de usuário do Docker Hub
- **Exemplo**: `rogerpereira` ou `crabcache`

### 2. DOCKER_PASSWORD
- **Valor**: Sua senha do Docker Hub ou Personal Access Token (recomendado)
- **Recomendação**: Use um Personal Access Token em vez da senha para maior segurança

## Como Configurar as Secrets

### Passo 1: Acesse as Configurações do Repositório
1. Vá para o seu repositório no GitHub
2. Clique em **Settings** (Configurações)
3. No menu lateral esquerdo, clique em **Secrets and variables**
4. Clique em **Actions**

### Passo 2: Adicione as Secrets
1. Clique em **New repository secret**
2. Adicione cada secret:

**Para DOCKER_USERNAME:**
- Name: `DOCKER_USERNAME`
- Secret: `seu-usuario-docker-hub`

**Para DOCKER_PASSWORD:**
- Name: `DOCKER_PASSWORD`
- Secret: `sua-senha-ou-token-docker-hub`

## Criando um Personal Access Token no Docker Hub (Recomendado)

1. Faça login no [Docker Hub](https://hub.docker.com/)
2. Vá para **Account Settings** → **Security**
3. Clique em **New Access Token**
4. Dê um nome descritivo (ex: "GitHub Actions Deploy")
5. Selecione as permissões necessárias (Read, Write, Delete)
6. Copie o token gerado e use como `DOCKER_PASSWORD`

## Como Funciona o Deploy

O workflow agora está configurado para:

1. **Em qualquer push/PR**: Executar testes e build do Docker
2. **Apenas em push para main**: Fazer deploy automático para Docker Hub

### Tags Criadas Automaticamente
- `latest`: Sempre aponta para a versão mais recente da main
- `main-{sha}`: Tag específica com o hash do commit para rastreabilidade

### Plataformas Suportadas
- `linux/amd64`
- `linux/arm64`

## Verificando o Deploy

Após um push para main, você pode:

1. Verificar o status na aba **Actions** do GitHub
2. Confirmar a imagem no Docker Hub em `https://hub.docker.com/r/{seu-usuario}/crabcache`

## Exemplo de Uso da Imagem

```bash
# Puxar a versão mais recente
docker pull {seu-usuario}/crabcache:latest

# Puxar uma versão específica
docker pull {seu-usuario}/crabcache:main-abc1234

# Executar o container
docker run -p 7000:7000 -p 7001:7001 {seu-usuario}/crabcache:latest
```

## Troubleshooting

### Erro de Autenticação
- Verifique se as secrets `DOCKER_USERNAME` e `DOCKER_PASSWORD` estão corretas
- Se usando token, confirme que tem as permissões necessárias

### Erro de Push
- Verifique se o repositório Docker Hub existe
- Confirme que o usuário tem permissão de escrita no repositório

### Build Falha
- Verifique os logs na aba Actions
- Confirme que o Dockerfile está correto e funcional