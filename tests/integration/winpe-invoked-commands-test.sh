#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
actual="$(awk -f "$ROOT/tests/integration/winpe-invoked-commands.awk" <<'INPUT'
fc /b a b
if exist x certutil -hashfile x
for %%F in (a b) do powershell -File x
"%DRV%\..\unapproved.exe" a b
bv-file-compare.exe a b
"%DRV%\..\bv-file-compare.exe" a b
INPUT
)"
expected=$(printf '%s\n' fc certutil powershell 'drv%\..\unapproved.exe"' bv-file-compare.exe)
[[ "$actual" == "$expected" ]] || { printf 'FAIL: WinPE tokenizer lost an unprovided executable\n' >&2; exit 1; }
printf 'PASS: unprovided commands remain visible; only the fixed bundled comparator path is recognized\n'
