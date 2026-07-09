#!/usr/bin/env sh
set -eu

INSTALL_DIR="${MULDEX_INSTALL_DIR:-$HOME/.local/bin}"
TARGET="$INSTALL_DIR/muldex"

if [ -f "$TARGET" ]; then
  rm -f "$TARGET"
fi

echo "uninstall.result: ok"
echo "uninstall.note: PATH cleanup is manual if you changed shell profile entries"
