#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MANIFEST="$ROOT_DIR/runtime/QUICKJS"

URL="${1:-}"
SHA256="${2:-}"

if [[ -f "$MANIFEST" ]]; then
  while IFS='=' read -r key value; do
    case "$key" in
      QJS_LINUX_X64_URL)
        if [[ -z "$URL" ]]; then URL="${value//$'\r'/}"; fi
        ;;
      QJS_LINUX_X64_SHA256)
        if [[ -z "$SHA256" ]]; then SHA256="${value//$'\r'/}"; fi
        ;;
    esac
  done < "$MANIFEST"
fi

if [[ -z "$URL" || -z "$SHA256" ]]; then
  echo "Missing QJS_LINUX_X64_URL or QJS_LINUX_X64_SHA256 in runtime/QUICKJS" >&2
  exit 1
fi

RUNTIME_DIR="$ROOT_DIR/runtime"
mkdir -p "$RUNTIME_DIR"
TARGET="$RUNTIME_DIR/qjs"
TMP="$RUNTIME_DIR/qjs.tmp"

verify_hash() {
  local file="$1"
  if command -v sha256sum >/dev/null 2>&1; then
    echo "$SHA256  $file" | sha256sum -c --status
  else
    python3 -c "import hashlib, sys; sys.exit(0 if hashlib.sha256(open(sys.argv[1],'rb').read()).hexdigest().lower() == sys.argv[2].lower() else 1)" "$file" "$SHA256"
  fi
}

if [[ -f "$TARGET" ]] && verify_hash "$TARGET"; then
  chmod +x "$TARGET"
  echo "QuickJS already present and verified at $TARGET"
  exit 0
fi

echo "Downloading QuickJS from $URL"
curl -fsSL "$URL" -o "$TMP"
if ! verify_hash "$TMP"; then
  rm -f "$TMP"
  echo "SHA256 mismatch for downloaded QuickJS binary" >&2
  exit 1
fi

chmod +x "$TMP"
mv -f "$TMP" "$TARGET"
echo "QuickJS ready at $TARGET"

