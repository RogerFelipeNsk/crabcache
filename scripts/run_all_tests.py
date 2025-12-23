#!/usr/bin/env python3
"""
Script para executar todos os testes do CrabCache
"""

import subprocess
import sys
import time
from pathlib import Path

def run_command(cmd, description, cwd=None):
    """Executa comando e retorna resultado"""
    print(f"🔧 {description}...")
    
    try:
        result = subprocess.run(
            cmd, 
            shell=True, 
            capture_output=True, 
            text=True, 
            cwd=cwd,
            timeout=300  # 5 minutos timeout
        )
        
        if result.returncode == 0:
            print(f"✅ {description}: PASSOU")
            return True
        else:
            print(f"❌ {description}: FALHOU")
            print(f"   Erro: {result.stderr}")
            return False
            
    except subprocess.TimeoutExpired:
        print(f"⏰ {description}: TIMEOUT")
        return False
    except Exception as e:
        print(f"❌ {description}: ERRO - {e}")
        return False

def main():
    print("🦀 CrabCache - Executando Todos os Testes")
    print("=" * 50)
    
    # Muda para diretório do projeto
    project_dir = Path(__file__).parent.parent
    
    tests = []
    
    # 1. Testes de compilação
    print("\n📦 TESTES DE COMPILAÇÃO")
    print("-" * 30)
    
    tests.append(("cargo check", "Verificação de compilação", project_dir))
    tests.append(("cargo test --lib", "Testes unitários", project_dir))
    tests.append(("cargo clippy -- -D warnings", "Linting (clippy)", project_dir))
    
    # 2. Testes de exemplos
    print("\n🔧 TESTES DE EXEMPLOS")
    print("-" * 30)
    
    tests.append(("cargo run --example security_example", "Exemplo de segurança", project_dir))
    tests.append(("cargo run --example wal_example", "Exemplo WAL", project_dir))
    
    # 3. Build Docker
    print("\n🐳 BUILD DOCKER")
    print("-" * 30)
    
    tests.append(("docker build -t crabcache:test -f docker/Dockerfile .", "Build Docker", project_dir))
    
    # 4. Testes de integração
    print("\n🧪 TESTES DE INTEGRAÇÃO")
    print("-" * 30)
    
    # Atualiza imagem nos scripts de teste
    update_commands = [
        "sed -i '' 's/crabcache:latest-wal-async/crabcache:test/g' scripts/test_simple.py",
        "sed -i '' 's/crabcache:latest-wal-async/crabcache:test/g' scripts/test_wal_focused.py",
        "sed -i '' 's/crabcache:latest-security/crabcache:test/g' scripts/benchmark_complete.py"
    ]
    
    for cmd in update_commands:
        subprocess.run(cmd, shell=True, cwd=project_dir, capture_output=True)
    
    tests.append(("python3 scripts/test_simple.py", "Teste básico", project_dir))
    tests.append(("python3 scripts/test_wal_focused.py", "Teste WAL", project_dir))
    tests.append(("python3 scripts/test_security.py", "Teste de segurança", project_dir))
    
    # 5. Benchmark
    print("\n🚀 BENCHMARK")
    print("-" * 30)
    
    tests.append(("python3 scripts/benchmark_complete.py", "Benchmark completo", project_dir))
    
    # Executa todos os testes
    results = []
    
    for cmd, description, cwd in tests:
        result = run_command(cmd, description, cwd)
        results.append((description, result))
        
        # Pausa entre testes
        if not result:
            print(f"⚠️  Continuando apesar do erro em: {description}")
        
        time.sleep(1)
    
    # Resumo final
    print("\n" + "=" * 60)
    print("📋 RESUMO DOS TESTES")
    print("=" * 60)
    
    passed = sum(1 for _, result in results if result)
    total = len(results)
    
    for description, result in results:
        status = "✅ PASSOU" if result else "❌ FALHOU"
        print(f"{description:.<40} {status}")
    
    print("-" * 60)
    print(f"Total: {passed}/{total} ({passed/total:.1%})")
    
    if passed == total:
        print("\n🎉 TODOS OS TESTES PASSARAM!")
        print("✅ CrabCache está funcionando perfeitamente!")
        print("\n🚀 Pronto para produção:")
        print("   - Performance otimizada")
        print("   - Segurança implementada")
        print("   - WAL persistence funcionando")
        print("   - Observabilidade completa")
    else:
        failed = total - passed
        print(f"\n⚠️  {failed} teste(s) falharam")
        print("💡 Verifique os logs acima para detalhes")
    
    print(f"\n📚 Documentação disponível:")
    print(f"   - README.md (visão geral)")
    print(f"   - docs/SECURITY_SYSTEM.md (segurança)")
    print(f"   - docs/WAL_PERSISTENCE.md (persistência)")
    print(f"   - docs/CrabCache-ExecutionPlan.md (plano completo)")
    
    return 0 if passed == total else 1

if __name__ == "__main__":
    exit(main())