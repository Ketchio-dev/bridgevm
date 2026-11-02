#!/usr/bin/env bash
# Informational: list the largest tracked source files. The per-file line-count
# ratchet (scripts/refactor-budgets.tsv) was removed on 2026-11-02: it forced
# registry edits and merge conflicts on every change without catching bugs.
# Splitting a big file is a judgment call made when it helps, not a gate.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."
git ls-files '*.rs' '*.swift' '*.sh' '*.py' | xargs wc -l 2>/dev/null \
  | grep -v ' total$' | sort -rn | awk 'NR <= 10 { printf "%8d %s\n", $1, $2 }'
echo "PASS: size report only (no budgets)"
