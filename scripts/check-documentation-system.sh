#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MANIFEST="$ROOT/docs/document-manifest.tsv"

[[ -f "$MANIFEST" ]] || {
  echo "documentation manifest is missing: $MANIFEST" >&2
  exit 1
}

# Bash 3.2 treats an empty-array expansion as unbound under `set -u`; retain a
# sentinel so the checker runs on the macOS system Bash without weakening nounset.
seen_paths=("__bridgevm_document_manifest_sentinel__")
errors=0
line_number=0

is_seen() {
  local candidate="$1"
  local item
  for item in "${seen_paths[@]}"; do
    [[ "$item" == "$candidate" ]] && return 0
  done
  return 1
}

while IFS=$'\t' read -r path class topic superseded_by extra; do
  line_number=$((line_number + 1))
  if [[ $line_number -eq 1 ]]; then
    [[ "$path" == "path" && "$class" == "class" && "$topic" == "topic" && "$superseded_by" == "superseded_by" ]] || {
      echo "invalid documentation manifest header" >&2
      errors=$((errors + 1))
    }
    continue
  fi
  [[ -n "$path" ]] || continue
  [[ -z "${extra:-}" ]] || {
    echo "manifest line $line_number has extra columns" >&2
    errors=$((errors + 1))
  }
  case "$class" in
    current|active-plan|decision|historical-evidence|reference) ;;
    *)
      echo "manifest line $line_number has invalid class: $class" >&2
      errors=$((errors + 1))
      ;;
  esac
  [[ "$path" == docs/*.md || "$path" == STATUS.md ]] || {
    echo "manifest line $line_number has invalid path: $path" >&2
    errors=$((errors + 1))
  }
  [[ -f "$ROOT/$path" ]] || {
    echo "manifest path does not exist: $path" >&2
    errors=$((errors + 1))
  }
  if is_seen "$path"; then
    echo "duplicate manifest path: $path" >&2
    errors=$((errors + 1))
  fi
  seen_paths+=("$path")
  if [[ "$superseded_by" != "-" && ! -f "$ROOT/$superseded_by" ]]; then
    echo "superseding document does not exist: $path -> $superseded_by" >&2
    errors=$((errors + 1))
  fi
done < "$MANIFEST"

while IFS= read -r path; do
  if ! is_seen "$path"; then
    echo "unclassified Markdown document: $path" >&2
    errors=$((errors + 1))
  fi
done < <(cd "$ROOT" && rg --files docs -g '*.md' | LC_ALL=C sort)

if [[ $errors -ne 0 ]]; then
  echo "documentation system: FAIL ($errors error(s))" >&2
  exit 1
fi

printf 'documentation system: PASS (%d classified Markdown documents)\n' "$(( ${#seen_paths[@]} - 1 ))"
