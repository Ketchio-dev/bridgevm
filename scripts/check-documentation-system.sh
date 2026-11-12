#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

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

# Source files are cited the same way and go stale the same way, most often when
# a module is split from foo.rs into foo/mod.rs. The split path is not accepted as
# a match: a reader following the citation needs the path that actually exists.
# Only repo-rooted directories are checked, so citations of an external tree's own
# layout are left alone.
SOURCE = re.compile(r'`((?:crates|apps)/[A-Za-z0-9._/-]+\.(?:rs|swift))`')

broken = 0
for path in sorted(files):
    with open(path, encoding='utf-8', errors='replace') as handle:
        for number, line in enumerate(handle, 1):
            for match in SOURCE.finditer(line):
                reference = match.group(1)
                if os.path.exists(reference):
                    continue
                if '(new)' in line or 'planned' in line.lower():
                    continue
                print(
                    f'source reference does not exist and is not marked planned: '
                    f'{path}:{number} -> {reference}',
                    file=sys.stderr,
                )
                broken += 1
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
if [[ $link_errors -ne 0 ]]; then
  echo "documentation system: FAIL ($link_errors error(s))" >&2
  exit 1
fi

printf 'documentation system: PASS (relative links and script/source references checked)\n'
