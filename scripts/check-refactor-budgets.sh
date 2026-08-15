#!/usr/bin/env bash
# Enforce the structural-debt ratchet budgets in scripts/refactor-budgets.tsv:
# no listed file may exceed its recorded line-count or unsafe-site ceiling, and
# no tracked Rust file may be absent from the TSV. As extraction reduces a file,
# lower its ceiling; the check then locks in the gain. The `budgets` job in
# .github/workflows/ci.yml runs this on every push.
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

# Inner doc comments are the crate's own explanation of itself, not structural
# debt. Counting them would make the ratchet penalise documenting a crate, so
# they are excluded. Ordinary comments still count, because an oversized file
# padded with commentary is exactly what the ratchet exists to catch.
count_loc() {
  awk '
    header && /^[[:space:]]*\/\/!/ { next }
    header && /^[[:space:]]*$/ { header = 0; next }
    { header = 0 }
    { count++ }
    END { print count + 0 }
  ' header=1 "$1"
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
  loc="$(count_loc "$file")"
  unsafe="$(count_unsafe "$file")"
  flag=""
  if (( loc > max_loc )); then flag+=" LOC>ceiling"; status=1; fi
  if (( unsafe > max_unsafe )); then flag+=" UNSAFE>ceiling"; status=1; fi
  printf '%-44s %8s %8s %8s %8s%s\n' "$path" "$loc" "$max_loc" "$unsafe" "$max_unsafe" "$flag"
done < "$BUDGETS"

# A file the TSV never lists is unbounded, which quietly defeats the ratchet:
# a new 3000-line module used to pass this gate untouched. Every tracked .rs
# must therefore carry a ceiling.
unlisted=0
while IFS= read -r tracked; do
  if ! grep -qF "$(printf '%s\t' "$tracked")" "$BUDGETS"; then
    echo "FAIL: tracked Rust file has no structural budget: $tracked" >&2
    unlisted=$((unlisted + 1))
    status=1
  fi
done < <(git -C "$ROOT" ls-files '*.rs')
if (( unlisted != 0 )); then
  echo "FAIL: $unlisted file(s) missing from $BUDGETS; add each with its current size." >&2
fi

if (( status != 0 )); then
  echo "FAIL: a file exceeded its structural-debt budget; extract into modules rather than growing these files, or lower a ceiling only after a real reduction." >&2
  exit 1
fi
echo "PASS: all files within their structural-debt budgets"
