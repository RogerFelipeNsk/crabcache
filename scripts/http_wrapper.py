#!/usr/bin/env python3
"""
HTTP Wrapper para CrabCache
Converte chamadas HTTP em comandos TCP para o CrabCache
"""

from flask import Flask, request, jsonify
import socket
import json
import os
import sys

app = Flask(__name__)

# Configuração do CrabCache
CRABCACHE_HOST = os.environ.get('CRABCACHE_HOST', 'localhost')
CRABCACHE_PORT = int(os.environ.get('CRABCACHE_PORT', 7005))  # Usar porta do Docker por padrão

def send_crabcache_command(command: str) -> str:
    """Envia comando para o CrabCache via TCP"""
    try:
        sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        sock.settimeout(5)
        sock.connect((CRABCACHE_HOST, CRABCACHE_PORT))
        
        if not command.endswith('\r\n'):
            command += '\r\n'
        
        sock.send(command.encode())
        response = sock.recv(4096).decode().strip()
        sock.close()
        
        return response
    except Exception as e:
        return f"ERROR: {e}"

@app.route('/ping', methods=['GET', 'POST'])
def ping():
    """Health check"""
    response = send_crabcache_command("PING")
    return jsonify({
        "command": "PING",
        "response": response,
        "success": response == "PONG"
    })

@app.route('/put', methods=['POST'])
def put():
    """PUT key value [ttl]"""
    data = request.get_json()
    
    if not data or 'key' not in data or 'value' not in data:
        return jsonify({"error": "Missing key or value"}), 400
    
    key = data['key']
    value = data['value']
    ttl = data.get('ttl')
    
    if ttl:
        command = f"PUT {key} {value} {ttl}"
    else:
        command = f"PUT {key} {value}"
    
    response = send_crabcache_command(command)
    
    return jsonify({
        "command": command,
        "response": response,
        "success": response == "OK"
    })

@app.route('/get/<key>', methods=['GET'])
def get(key):
    """GET key"""
    command = f"GET {key}"
    response = send_crabcache_command(command)
    
    return jsonify({
        "command": command,
        "response": response,
        "success": response != "NULL",
        "value": response if response != "NULL" else None
    })

@app.route('/delete/<key>', methods=['DELETE'])
def delete(key):
    """DEL key"""
    command = f"DEL {key}"
    response = send_crabcache_command(command)
    
    return jsonify({
        "command": command,
        "response": response,
        "success": response == "OK"
    })

@app.route('/expire', methods=['POST'])
def expire():
    """EXPIRE key ttl"""
    data = request.get_json()
    
    if not data or 'key' not in data or 'ttl' not in data:
        return jsonify({"error": "Missing key or ttl"}), 400
    
    key = data['key']
    ttl = data['ttl']
    
    command = f"EXPIRE {key} {ttl}"
    response = send_crabcache_command(command)
    
    return jsonify({
        "command": command,
        "response": response,
        "success": response == "OK"
    })

@app.route('/stats', methods=['GET'])
def stats():
    """STATS"""
    command = "STATS"
    response = send_crabcache_command(command)
    
    # Parse stats response
    stats_data = {}
    if response.startswith("STATS:"):
        stats_text = response[6:].strip()
        # Parse shard info
        parts = stats_text.split(", ")
        for part in parts:
            if ":" in part and ("shard_" in part or "total:" in part):
                key_part, value_part = part.split(":", 1)
                stats_data[key_part.strip()] = value_part.strip()
    
    return jsonify({
        "command": command,
        "response": response,
        "success": response.startswith("STATS:"),
        "parsed": stats_data
    })

@app.route('/command', methods=['POST'])
def raw_command():
    """Enviar comando raw"""
    data = request.get_json()
    
    if not data or 'command' not in data:
        return jsonify({"error": "Missing command"}), 400
    
    command = data['command']
    response = send_crabcache_command(command)
    
    return jsonify({
        "command": command,
        "response": response
    })

@app.route('/health', methods=['GET'])
def health():
    """Health check do wrapper"""
    crabcache_response = send_crabcache_command("PING")
    
    return jsonify({
        "wrapper": "OK",
        "crabcache": crabcache_response,
        "healthy": crabcache_response == "PONG"
    })

@app.route('/', methods=['GET'])
def index():
    """Documentação da API"""
    return jsonify({
        "name": "CrabCache HTTP Wrapper",
        "version": "1.0.0",
        "endpoints": {
            "GET /": "Esta documentação",
            "GET /health": "Health check",
            "GET /ping": "PING do CrabCache",
            "POST /put": "PUT key/value (JSON: {key, value, ttl?})",
            "GET /get/<key>": "GET key",
            "DELETE /delete/<key>": "DEL key", 
            "POST /expire": "EXPIRE key (JSON: {key, ttl})",
            "GET /stats": "STATS do servidor",
            "POST /command": "Comando raw (JSON: {command})"
        },
        "examples": {
            "put": {"key": "user:123", "value": "john_doe", "ttl": 3600},
            "expire": {"key": "user:123", "ttl": 1800},
            "command": {"command": "PING"}
        }
    })

if __name__ == '__main__':
    import os
    
    print("🌐 CrabCache HTTP Wrapper")
    print("=" * 40)
    print(f"CrabCache: {CRABCACHE_HOST}:{CRABCACHE_PORT}")
    print("HTTP API: http://0.0.0.0:8000")
    print("Documentação: http://localhost:8000/")
    print("=" * 40)
    
    # Usar Gunicorn em produção se disponível
    flask_env = os.environ.get('FLASK_ENV', 'development')
    
    if flask_env == 'production':
        try:
            import gunicorn.app.wsgiapp as wsgi
            print("🚀 Iniciando com Gunicorn (produção)")
            # Configurar Gunicorn programaticamente
            sys.argv = [
                'gunicorn',
                '--bind', '0.0.0.0:8000',
                '--workers', '2',
                '--worker-class', 'sync',
                '--timeout', '30',
                '--max-requests', '1000',
                '--max-requests-jitter', '100',
                '--access-logfile', '-',
                '--error-logfile', '-',
                'http_wrapper:app'
            ]
            wsgi.run()
        except ImportError:
            print("⚠️  Gunicorn não disponível, usando Flask dev server")
            app.run(host='0.0.0.0', port=8000, debug=False)
        except Exception as e:
            print(f"❌ Erro no Gunicorn: {e}")
            print("🔄 Fallback para Flask dev server")
            app.run(host='0.0.0.0', port=8000, debug=False)
    else:
        print("🔧 Modo desenvolvimento")
        app.run(host='0.0.0.0', port=8000, debug=True)