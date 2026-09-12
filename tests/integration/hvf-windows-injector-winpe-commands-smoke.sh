#!/usr/bin/env bash
# Every external command bvinject.cmd invokes must exist inside WinPE.
#
# bvinject.cmd runs from winpeshl.ini in boot.wim image 2 (Windows PE arm64
# 10.0.26100). That image has no fc.exe, comp.exe, findstr.exe, certutil.exe or
# powershell.exe. From db81c72c the plant block verified a copy with `fc /b`;
# cmd returned 9009, `if errorlevel 1` took the error path, and the block left
# before the firstboot pending flag and activation service were planted. Live
# job t7-c193b7c0-fixed-b6-observation-r1 then booted a guest on which
# firstboot never ran. The allowed set below is the intersection of what the
# injector needs and what was measured present in the sealed injector image
# (wimlib-imagex dir, /Windows/System32).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
INJECTOR="$ROOT/scripts/win-assets/bvinject.cmd"
[[ -f "$INJECTOR" ]] || { echo "FAIL: missing $INJECTOR" >&2; exit 1; }

# cmd.exe builtins plus WinPE System32 executables the injector may call.
ALLOWED='call copy del dir echo exit for goto if md mkdir rd rem rmdir set setlocal endlocal type dism find reg wpeinit wpeutil xcopy'

# Strip CRLF, comments, labels and block closers, then peel every leading
# `if [not] exist X` / `if errorlevel N` / `if defined X` / `if "a"=="b"` /
# `for ... do` prefix so the word left is the command actually executed.
python3 "$ROOT/tests/integration/injector-file-compare-wiring-contract.py"
bash "$ROOT/tests/integration/winpe-invoked-commands-test.sh"
invoked="$(tr -d '\r' < "$INJECTOR" | awk -f "$ROOT/tests/integration/winpe-invoked-commands.awk" | sort -u)"

violations=""
for word in $invoked; do
  [[ " $ALLOWED " == *" $word "* ]] || violations="$violations $word"
done

if [[ -n "$violations" ]]; then
  echo "FAIL: bvinject.cmd invokes commands that WinPE does not ship:$violations" >&2
  exit 1
fi
echo "PASS: bvinject.cmd invokes only WinPE-resident commands"
