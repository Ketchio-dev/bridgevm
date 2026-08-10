#!/usr/bin/env bash
set -euo pipefail
inventory="${1:-}"
bundle="${2:-}"
[[ -n "$bundle" && -s "$bundle" ]] || { echo "bundled Rust license texts are missing" >&2; exit 1; }
expected=$(( $(wc -l < "$inventory") - 3 ))
actual="$(grep -c '^=== .* ===$' "$bundle")"
[[ "$actual" == "$expected" ]] || {
  echo "Rust inventory/license package count mismatch: $expected != $actual" >&2
  exit 1
}
