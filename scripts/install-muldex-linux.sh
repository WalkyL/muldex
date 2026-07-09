#!/usr/bin/env sh
set -eu

BINARY_PATH="${1:-./target/debug/muldex}"
INSTALL_DIR="${MULDEX_INSTALL_DIR:-$HOME/.local/bin}"
CONFIG_PATH="${MULDEX_CONFIG_PATH:-$HOME/.muldex/config.json}"
USE_LLM_ROUTER="${MULDEX_USE_LLM_ROUTER:-}"
SKIP_LLM_ROUTER_PROMPT="${MULDEX_SKIP_LLM_ROUTER_PROMPT:-}"

if [ ! -f "$BINARY_PATH" ]; then
  echo "binary not found: $BINARY_PATH" >&2
  exit 1
fi

mkdir -p "$INSTALL_DIR"
cp "$BINARY_PATH" "$INSTALL_DIR/muldex"
chmod +x "$INSTALL_DIR/muldex"

enable_llm_router="$USE_LLM_ROUTER"

if [ -z "$SKIP_LLM_ROUTER_PROMPT" ] && [ -z "$USE_LLM_ROUTER" ]; then
  echo
  echo "muldex uses an OpenAI-compatible request shape."
  echo "Many model providers are not fully compatible with that shape in practice."
  echo "llm-router is the recommended compatibility layer for request and response normalization."
  printf 'Configure llm-router as the default provider now? [Y/n] '
  read -r choice
  if [ -z "$choice" ] || [ "$choice" = "y" ] || [ "$choice" = "Y" ] || [ "$choice" = "yes" ] || [ "$choice" = "YES" ]; then
    enable_llm_router=1
  fi
fi

if [ -n "$enable_llm_router" ]; then
  printf 'llm-router host/IP [127.0.0.1] '
  read -r router_host
  if [ -z "$router_host" ]; then
    router_host='127.0.0.1'
  fi

  printf 'llm-router port [3000] '
  read -r router_port
  if [ -z "$router_port" ]; then
    router_port='3000'
  fi

  printf 'llm-router API key (leave blank to set later) '
  read -r router_api_key
  printf 'default model (optional, leave blank to set later) '
  read -r router_default_model

  mkdir -p "$(dirname "$CONFIG_PATH")"
  python - <<'PY' "$CONFIG_PATH" "$router_host" "$router_port" "$router_api_key" "$router_default_model"
import json
import os
import sys

path, host, port, api_key, default_model = sys.argv[1:6]
if os.path.exists(path):
    with open(path, 'r', encoding='utf-8') as fh:
        config = json.load(fh)
else:
    config = {
        'schema_version': 'muldex-config-v1',
        'default_provider': 'llm-router',
        'providers': {},
    }

config.setdefault('schema_version', 'muldex-config-v1')
config['default_provider'] = 'llm-router'
config.setdefault('providers', {})
config['providers']['llm-router'] = {
    'kind': 'openai-compatible',
    'host': host,
    'port': int(port),
    'api_key': api_key,
    'default_model': default_model or None,
}

with open(path, 'w', encoding='utf-8') as fh:
    json.dump(config, fh, indent=2)
PY

  connectivity=$(python - <<'PY' "$router_host" "$router_port"
import socket
import sys

host, port = sys.argv[1], int(sys.argv[2])
sock = socket.socket()
sock.settimeout(2)
try:
    sock.connect((host, port))
    print('reachable')
except socket.timeout:
    print('timeout')
except Exception as exc:
    print(f'unreachable: {exc}')
finally:
    sock.close()
PY
)
fi

echo "install.result: ok"
echo "install.binary: $INSTALL_DIR/muldex"
if [ -n "$enable_llm_router" ]; then
  echo "install.llm_router: configured"
  echo "install.config_path: $CONFIG_PATH"
  echo "install.llm_router.connectivity: $connectivity"
  echo "install.next_step: verify with /config llm test or /provider test inside the shell"
else
  echo "install.llm_router: skipped"
  echo "install.next_step: manually configure a provider before normal shell use"
fi
echo "install.note: ensure $INSTALL_DIR is on PATH"
