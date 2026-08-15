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

# One awk pass over the TSV and every budgeted file. Spawning awk/grep/wc/tr per
# row meant 5,256 processes for 1,314 rows and cost 5.9 s of this gate's 10.4 s.
# Inner doc comments are excluded so the ratchet does not penalise documenting a
# crate; ordinary comments count, since padding a file with commentary is what
# it exists to catch. unsafe sites are unsafe fn/impl/block/extern everywhere.
set +e
report=$(awk -v root="$ROOT" '
  BEGIN { FS = "\t"; printf "%-44s %8s %8s %8s %8s\n", "file", "loc", "loc_max", "unsafe", "uns_max" }
  /^#/ || /^[[:space:]]*$/ { next }
  {
    path = $1; max_loc = $2 + 0; max_unsafe = $3 + 0
    file = root "/" path
    if ((getline probe < file) < 0) {
      printf "FAIL: budgeted file does not exist: %s\n", path > "/dev/stderr"
      status = 1
      next
    }
    close(file)
    loc = 0; unsafe_sites = 0; header = 1
    while ((getline line < file) > 0) {
      if (header && line ~ /^[[:space:]]*\/\/!/) continue
      if (header && line ~ /^[[:space:]]*$/) { header = 0; continue }
      header = 0
      loc++
      rest = line
      while (match(rest, /unsafe (fn|impl|\{|extern)/)) {
        unsafe_sites++
        rest = substr(rest, RSTART + RLENGTH)
      }
    }
    close(file)
    flag = ""
    if (loc > max_loc) { flag = flag " LOC>ceiling"; status = 1 }
    if (unsafe_sites > max_unsafe) { flag = flag " UNSAFE>ceiling"; status = 1 }
    printf "%-44s %8d %8d %8d %8d%s\n", path, loc, max_loc, unsafe_sites, max_unsafe, flag
  }
  END { exit status }
' "$BUDGETS")
status=$?
set -e
printf '%s\n' "$report"

# An unlisted file is unbounded, which defeats the ratchet: a new 3000-line
# module used to pass untouched. comm over sorted lists, not a grep per file
# (3.5 s alone).
unlisted="$(comm -23 \
  <(git -C "$ROOT" ls-files '*.rs' '*.swift' '*.sh' '*.py' | sort) \
  <(grep -v '^#' "$BUDGETS" | cut -f1 | sort))"
if [[ -n "$unlisted" ]]; then
  sed 's/^/FAIL: tracked source file has no structural budget: /' <<< "$unlisted" >&2
  echo "FAIL: $(wc -l <<< "$unlisted" | tr -d ' ') file(s) missing from $BUDGETS." >&2
  status=1
fi

if (( status != 0 )); then
  echo "FAIL: a file exceeded its structural-debt budget; extract into modules rather than growing these files, or lower a ceiling only after a real reduction." >&2
  exit 1
fi
echo "PASS: all files within their structural-debt budgets"
