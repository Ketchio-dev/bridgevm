#!/usr/bin/env bash
# The closure tier must end even when F4 left Notepad with a modified document.
#
# Live job t7-7f31bfc8-keeprunning-b6-observation-r1: after the F4 key batches,
# WM_CLOSE raised the save prompt and the window stayed as "*Untitled -
# Notepad"; the tier's plain `shutdown /s /t 0` was then vetoed and the guest
# idled at the desktop for 50 minutes until the 3000 s watchdog cancelled the
# run. The discard is a separate CRLF guest script, shared like the other
# closure assets, and runs only after F3 has already been judged, so a stuck
# window can never be reported as a Coherence pass.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
INTERACT="$ROOT/scripts/windows-1.0-closure-interact.sh"
DISCARD="$ROOT/scripts/win-assets/bv-windows-closure-discard.ps1"
fail() { echo "FAIL: $*" >&2; exit 1; }

[[ "$(grep -c 'shutdown /s /f /t 0' "$INTERACT")" -eq 2 && "$(grep -c 'shutdown /s /t 0' "$INTERACT")" -eq 0 ]] \
  || fail 'closure shutdown must be forced on both the normal and cleanup paths'
pass_line=$(grep -n '^      f3=pass$' "$INTERACT" | cut -d: -f1)
discard_line=$(grep -n 'bv-windows-closure-discard.ps1 -Hwnd' "$INTERACT" | head -n 1 | cut -d: -f1)
[[ -n "$pass_line" && -n "$discard_line" && "$pass_line" -lt "$discard_line" ]] \
  || fail 'discard must run only after F3 has been judged'
grep -Eq 'bv-windows-closure-discard.ps1" "\$OUT/share/"' "$INTERACT" || fail 'discard script is not shared'
grep -Eq 'bv-windows-closure-launch.ps1 bv-windows-closure-discard.ps1; do' "$INTERACT" || fail 'discard share is not awaited'
grep -Eq 'Stop-Process -Id \$owner -Force' "$DISCARD" || fail 'discard does not end the owning process'
grep -Eq 'BVDISCARD hwnd=' "$DISCARD" || fail 'discard does not report what it did'
python3 -c 'import pathlib,sys; p=pathlib.Path(sys.argv[1]).read_bytes(); sys.exit(0 if p.count(b"\n") == p.count(b"\r\n") and b"\n" in p else 1)' "$DISCARD" \
  || fail 'discard script must use CRLF line endings'
echo 'PASS: Windows closure discard policy smoke'
