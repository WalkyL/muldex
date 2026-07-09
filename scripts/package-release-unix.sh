#!/usr/bin/env sh
set -eu

TARGET="${1:?target required}"
ARTIFACT_NAME="${2:?artifact name required}"

REPO_ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
BINARY_PATH="$REPO_ROOT/target/$TARGET/release/muldex"
PACKAGE_ROOT="$REPO_ROOT/target/release-package/$ARTIFACT_NAME"
PACKAGE_ARCHIVE="$PACKAGE_ROOT.tar.gz"

if [ ! -f "$BINARY_PATH" ]; then
  echo "binary not found: $BINARY_PATH" >&2
  exit 1
fi

rm -rf "$PACKAGE_ROOT"
rm -f "$PACKAGE_ARCHIVE"

mkdir -p "$PACKAGE_ROOT"
cp "$BINARY_PATH" "$PACKAGE_ROOT/muldex"
chmod +x "$PACKAGE_ROOT/muldex"

if printf '%s' "$TARGET" | grep -q 'apple-darwin'; then
  cp "$REPO_ROOT/scripts/install-muldex-macos.sh" "$PACKAGE_ROOT/install.sh"
  cp "$REPO_ROOT/scripts/uninstall-muldex-macos.sh" "$PACKAGE_ROOT/uninstall.sh"
else
  cp "$REPO_ROOT/scripts/install-muldex-linux.sh" "$PACKAGE_ROOT/install.sh"
  cp "$REPO_ROOT/scripts/uninstall-muldex-linux.sh" "$PACKAGE_ROOT/uninstall.sh"
fi

cat > "$PACKAGE_ROOT/README.txt" <<EOF
muldex release artifact: $ARTIFACT_NAME

docs:
- docs/interactive-shell-guide.md
- docs/interactive-shell-validation.md
- docs/interactive-shell-release-checklist.md
- docs/installing-muldex-cli.md
EOF

tar -C "$(dirname "$PACKAGE_ROOT")" -czf "$PACKAGE_ARCHIVE" "$(basename "$PACKAGE_ROOT")"

echo "package.result: ok"
echo "package.path: $PACKAGE_ARCHIVE"
