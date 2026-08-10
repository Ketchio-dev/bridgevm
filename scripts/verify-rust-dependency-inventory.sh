#!/usr/bin/env bash
# Validate the machine-readable Rust inventory embedded in a release app.
set -euo pipefail

inventory="${1:-}"
[[ -n "$inventory" && -s "$inventory" ]] || {
  echo "bundled Rust dependency inventory is missing" >&2
  exit 1
}
grep -Fx 'format=bridgevm-rust-dependencies-v1' "$inventory" >/dev/null || {
  echo "bundled Rust dependency inventory has an unsupported format" >&2
  exit 1
}
if grep -F $'\t<missing>\t' "$inventory" >/dev/null; then
  echo "bundled Rust dependency inventory contains missing licenses" >&2
  exit 1
fi
count=$(( $(wc -l < "$inventory") - 3 ))
[[ "$count" -gt 0 ]] || {
  echo "bundled Rust dependency inventory is empty" >&2
  exit 1
}
printf 'rust_dependency_count=%s\n' "$count"
