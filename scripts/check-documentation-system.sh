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
  # GOAL.md is an operator ledger kept out of git on purpose, so it is a valid
  # superseding target that a fresh CI checkout will not contain.
  if [[ "$superseded_by" != "-" && "$superseded_by" != "GOAL.md" && ! -f "$ROOT/$superseded_by" ]]; then
    echo "superseding document does not exist: $path -> $superseded_by" >&2
    errors=$((errors + 1))
  fi
done < "$MANIFEST"

while IFS= read -r path; do
  if ! is_seen "$path"; then
    echo "unclassified Markdown document: $path" >&2
    errors=$((errors + 1))
  fi
done < <(cd "$ROOT" && find docs -type f -name '*.md' | LC_ALL=C sort)

# Relative links must resolve, so a moved or renamed file cannot leave a
# document pointing at nothing. docs/archive holds point-in-time snapshots whose
# links describe the tree as it was, so rewriting them would falsify the record.
link_errors=$(cd "$ROOT" && python3 - <<'PY'
import os
import re
import sys

LINK = re.compile(r'\[[^\]]*\]\(([^)]+)\)')
# Inline code can contain bracket-paren sequences that are not links, such as a
# PowerShell cast like `![bool](Get-Tpm).OwnerAuth`.
CODE = re.compile(r'`[^`]*`')
roots = ['docs']
extra = ['README.md', 'STATUS.md', 'AGENTS.md', 'THIRD-PARTY-NOTICES.md']

files = []
for root in roots:
    for dirpath, _dirnames, filenames in os.walk(root):
        if 'archive' in dirpath.split(os.sep):
            continue
        files.extend(
            os.path.join(dirpath, name)
            for name in filenames
            if name.endswith('.md')
        )
files.extend(name for name in extra if os.path.exists(name))
files.extend(name for name in ('PLAN.md',) if os.path.exists(name))

# A script named in backticks must either exist or be marked as planned, so a
# reader can never mistake an unwritten script for one they can run.
SCRIPT = re.compile(r'`((?:scripts|tools)/[A-Za-z0-9._/-]+\.(?:sh|py))`')

broken = 0
for path in sorted(files):
    with open(path, encoding='utf-8', errors='replace') as handle:
        for number, line in enumerate(handle, 1):
            for match in SCRIPT.finditer(line):
                script = match.group(1)
                if os.path.exists(script):
                    continue
                if '(new)' in line or 'planned' in line.lower():
                    continue
                print(
                    f'script reference does not exist and is not marked planned: '
                    f'{path}:{number} -> {script}',
                    file=sys.stderr,
                )
                broken += 1
            line = CODE.sub(lambda m: ' ' * len(m.group(0)), line)
            for match in LINK.finditer(line):
                target = match.group(1).split('#')[0].strip()
                if not target or target.startswith(('http://', 'https://', 'mailto:', '#')):
                    continue
                if target.startswith('<') and target.endswith('>'):
                    target = target[1:-1]
                resolved = os.path.normpath(os.path.join(os.path.dirname(path), target))
                if not os.path.exists(resolved):
                    print(f'broken link: {path}:{number} -> {target}', file=sys.stderr)
                    broken += 1
print(broken)
PY
)
errors=$((errors + link_errors))

if [[ $errors -ne 0 ]]; then
  echo "documentation system: FAIL ($errors error(s))" >&2
  exit 1
fi

printf 'documentation system: PASS (%d classified Markdown documents, %d link targets resolved)\n' "$(( ${#seen_paths[@]} - 1 ))" "$(cd "$ROOT" && grep -ro '](\([^)]*\))' --include='*.md' docs README.md STATUS.md 2>/dev/null | wc -l | tr -d ' ')"
