#!/usr/bin/env bash
# Discover and preflight candidate Windows ARM64 viogpu3d driver packages.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CHECK_PACKAGE="${CHECK_PACKAGE:-"$ROOT/scripts/check-hvf-windows-viogpu3d-package.sh"}"
OUT_DIR="${OUT_DIR:-}"
MAX_DEPTH="${MAX_DEPTH:-8}"
REQUIRE_FOUND="${REQUIRE_FOUND:-0}"

usage() {
  cat >&2 <<'EOF'
usage: scripts/find-hvf-windows-viogpu3d-packages.sh [--root DIR ...] [--out-dir DIR] [--require-found]

Options:
  --root DIR        Root to scan. Repeatable. Default: $HOME/BridgeVM.
  --out-dir DIR     Directory for inventory.txt and per-candidate manifests.
                    Default: /tmp/bridgevm-viogpu3d-inventory.<pid>.
  --max-depth N     find(1) depth limit. Default: 8.
  --require-found   Exit non-zero when no injection-ready package is found.

The scanner looks for directories hinted by viogpu3d filenames or viogpu3d INFs
that advertise PCI\VEN_1AF4&DEV_1050 or PCI\VEN_1AF4&DEV_10F7, then runs the
package checker against each candidate. It never mutates the scanned roots.
EOF
}

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

positive_integer() {
  case "$1" in
    ''|*[!0-9]*) return 1 ;;
  esac
  (( "$1" > 0 ))
}

sanitize_name() {
  printf '%s\n' "$1" | sed 's#[^A-Za-z0-9._-]#_#g'
}

append_candidate() {
  local path="$1"
  [[ -d "$path" ]] || return 0
  printf '%s\n' "$path" >> "$CANDIDATES_RAW"
}

roots=()
while [[ $# -gt 0 ]]; do
  case "$1" in
    --root)
      [[ $# -ge 2 ]] || { usage; exit 2; }
      roots+=("$2")
      shift 2
      ;;
    --out-dir)
      [[ $# -ge 2 ]] || { usage; exit 2; }
      OUT_DIR="$2"
      shift 2
      ;;
    --max-depth)
      [[ $# -ge 2 ]] || { usage; exit 2; }
      positive_integer "$2" || fail "--max-depth requires a positive integer"
      MAX_DEPTH="$2"
      shift 2
      ;;
    --require-found)
      REQUIRE_FOUND="1"
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      usage
      exit 2
      ;;
  esac
done

case "$REQUIRE_FOUND" in
  0|1) ;;
  *) fail "REQUIRE_FOUND must be 0 or 1" ;;
esac

if (( ${#roots[@]} == 0 )); then
  roots=("$HOME/BridgeVM")
fi
if [[ -z "$OUT_DIR" ]]; then
  OUT_DIR="/tmp/bridgevm-viogpu3d-inventory.$$"
fi
mkdir -p "$OUT_DIR"

CANDIDATES_RAW="$OUT_DIR/candidates.raw"
CANDIDATES="$OUT_DIR/candidates.txt"
INVENTORY="$OUT_DIR/inventory.txt"
: > "$CANDIDATES_RAW"

for root_dir in "${roots[@]}"; do
  if [[ ! -d "$root_dir" ]]; then
    printf 'missing_root=%s\n' "$root_dir" >> "$INVENTORY.missing"
    continue
  fi

  while IFS= read -r file; do
    [[ -n "$file" ]] || continue
    case "$(basename "$file" | tr '[:upper:]' '[:lower:]')" in
      *.inf|*.inx)
        base="$(basename "$file" | tr '[:upper:]' '[:lower:]')"
        if [[ "$base" == *viogpu3d* ]] &&
          grep -Eiq 'VEN_1AF4.*DEV_(1050|10F7)|DEV_(1050|10F7).*VEN_1AF4' "$file" 2>/dev/null
        then
          append_candidate "$(dirname "$file")"
        fi
        ;;
      *viogpu*3d*.sys|*viogpu3d*.sys)
        append_candidate "$(dirname "$file")"
        ;;
    esac
  done < <(
    find "$root_dir" -maxdepth "$MAX_DEPTH" -type f \
      \( -iname '*.inf' -o -iname '*.inx' -o -iname '*viogpu*3d*.sys' -o -iname '*viogpu3d*.sys' \) \
      -print 2>/dev/null
  )
done

sort -u "$CANDIDATES_RAW" > "$CANDIDATES"

ready_count=0
candidate_count=0
{
  printf 'BridgeVM viogpu3d package inventory\n'
  printf 'generated_utc=%s\n' "$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
  printf 'out_dir=%s\n' "$OUT_DIR"
  printf 'max_depth=%s\n' "$MAX_DEPTH"
  for root_dir in "${roots[@]}"; do
    printf 'root=%s\n' "$root_dir"
  done
  if [[ -f "$INVENTORY.missing" ]]; then
    cat "$INVENTORY.missing"
  fi

  while IFS= read -r candidate; do
    [[ -n "$candidate" ]] || continue
    candidate_count=$((candidate_count + 1))
    name="$(sanitize_name "$candidate")"
    manifest="$OUT_DIR/candidate-$candidate_count-$name-manifest.txt"
    log="$OUT_DIR/candidate-$candidate_count-$name-check.txt"
    printf 'candidate=%s\n' "$candidate"
    printf 'candidate_manifest=%s\n' "$manifest"
    printf 'candidate_check_log=%s\n' "$log"
    if "$CHECK_PACKAGE" --manifest "$manifest" "$candidate" > "$log" 2>&1; then
      ready_count=$((ready_count + 1))
      protocol="$(awk -F= '$1 == "protocol" { print $2; exit }' "$log")"
      printf 'candidate_status=ready\n'
      printf 'candidate_protocol=%s\n' "${protocol:-unknown}"
    else
      printf 'candidate_status=rejected\n'
      first_fail="$(awk '/^FAIL:/ { print; exit }' "$log")"
      printf 'candidate_reject_reason=%s\n' "${first_fail:-see check log}"
    fi
  done < "$CANDIDATES"

  printf 'candidate_count=%s\n' "$candidate_count"
  printf 'ready_count=%s\n' "$ready_count"
  if (( ready_count == 0 )); then
    printf 'Blocker: no injection-ready viogpu3d package found\n'
  else
    printf 'PASS: injection-ready viogpu3d package found\n'
  fi
} > "$INVENTORY"

cat "$INVENTORY"

if [[ "$REQUIRE_FOUND" == "1" && "$ready_count" == "0" ]]; then
  exit 1
fi
