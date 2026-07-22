#!/usr/bin/env bash
# Enforce the structural-debt ratchet budgets in scripts/refactor-budgets.tsv:
# no listed file may exceed its recorded line-count or unsafe-site ceiling. This
# stands in for a CI budget gate (the repository has no hosted CI). As extraction
# reduces a file, lower its ceiling in the TSV; the check then locks in the gain.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BUDGETS="$ROOT/scripts/refactor-budgets.tsv"

[[ -f "$BUDGETS" ]] || { echo "FAIL: missing budgets file: $BUDGETS" >&2; exit 1; }

# Count unsafe sites the same way across the codebase: unsafe fn/impl/block/extern.
count_unsafe() {
  # `|| true`: grep exits 1 on a file with zero unsafe sites, which would
  # otherwise trip `set -o pipefail`.
  { grep -oE 'unsafe (fn|impl|\{|extern)' "$1" || true; } | wc -l | tr -d ' '
}

status=0
printf '%-44s %8s %8s %8s %8s\n' "file" "loc" "loc_max" "unsafe" "uns_max"
while IFS=$'\t' read -r path max_loc max_unsafe; do
  [[ -z "${path:-}" || "${path:0:1}" == "#" ]] && continue
  file="$ROOT/$path"
  if [[ ! -f "$file" ]]; then
    echo "FAIL: budgeted file does not exist: $path" >&2
    status=1
    continue
  fi
  loc="$(wc -l < "$file" | tr -d ' ')"
  unsafe="$(count_unsafe "$file")"
  flag=""
  if (( loc > max_loc )); then flag+=" LOC>ceiling"; status=1; fi
  if (( unsafe > max_unsafe )); then flag+=" UNSAFE>ceiling"; status=1; fi
  printf '%-44s %8s %8s %8s %8s%s\n' "$path" "$loc" "$max_loc" "$unsafe" "$max_unsafe" "$flag"
done < "$BUDGETS"

if (( status != 0 )); then
  echo "FAIL: a file exceeded its structural-debt budget; extract into modules rather than growing these files, or lower a ceiling only after a real reduction." >&2
  exit 1
fi
echo "PASS: all files within their structural-debt budgets"
