#!/usr/bin/env bash
# Optimized wasm build; prints hash and size.
set -euo pipefail
cd "$(dirname "$0")/.."

# Print the SHA-256 hex digest of a file. Prefers sha256sum (GNU coreutils,
# common on Linux/CI) and falls back to shasum -a 256 (macOS, perl-based).
# Both print "<hash>  <file>", so the first field is taken.
sha256_of() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | cut -d' ' -f1
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$1" | cut -d' ' -f1
  else
    echo "error: neither sha256sum nor shasum is available; install coreutils or perl to hash $1" >&2
    return 1
  fi
}

stellar contract build --locked
W=target/wasm32v1-none/release/bounty_escrow.wasm
if command -v wasm-opt >/dev/null 2>&1; then
  wasm-opt -Oz "$W" -o "$W"
fi
echo "size: $(wc -c < "$W") bytes"
hash="$(sha256_of "$W")"
echo "sha256: $hash"
