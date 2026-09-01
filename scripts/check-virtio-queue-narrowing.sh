#!/usr/bin/env bash
# Reject narrowing a guest register before applying its advertised virtio limit.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

pattern='\([^()]*as u(16|32)\)\.min\('
bad='let size = (value as u16).min(QUEUE_MAX);'
good='let size = value.min(u64::from(QUEUE_MAX)) as u16;'

printf '%s\n' "$bad" | rg -q "$pattern" || {
  echo "virtio queue narrowing: FAIL (guard did not reject bad fixture)" >&2
  exit 1
}
if printf '%s\n' "$good" | rg -q "$pattern"; then
  echo "virtio queue narrowing: FAIL (guard rejected good fixture)" >&2
  exit 1
fi

if matches="$(rg -n --glob '*.rs' --glob '!**/tests/**' "$pattern" \
    crates/bridgevm-hvf/src/virtio_* 2>/dev/null)"; then
  printf 'virtio queue narrowing: FAIL (cast before clamp)\n%s\n' "$matches" >&2
  exit 1
fi

echo "virtio queue narrowing: PASS"
