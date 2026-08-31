#!/usr/bin/env bash
# Prove that two separate BridgeVM processes preserve one validated vars file.
set -euo pipefail

[[ $# -eq 3 ]] || { echo "usage: $0 BINARY FIRMWARE_FD WORK_DIRECTORY" >&2; exit 2; }
BIN="$1"
FD="$2"
WORK="$3"
VARS="$WORK/BridgeVmPcVars.fd"

WRITTEN_OUT="$("$BIN" "$FD" --vars-file "$VARS" --expect written)"
echo "$WRITTEN_OUT"
echo "$WRITTEN_OUT" | grep -q "process_mode=written"
echo "$WRITTEN_OUT" | grep -Eq "dxe_result=10 .*variable_state=1"
WRITTEN_HASH="$(awk -F= '/^vars_file_sha256=/{print $2}' <<<"$WRITTEN_OUT")"
[[ ${#WRITTEN_HASH} -eq 64 ]]
[[ "$(stat -f %z "$VARS")" -eq 65536 ]]

RESTORED_OUT="$("$BIN" "$FD" --vars-file "$VARS" --expect restored)"
echo "$RESTORED_OUT"
echo "$RESTORED_OUT" | grep -q "process_mode=restored"
echo "$RESTORED_OUT" | grep -Eq "dxe_result=11 .*variable_state=2"
RESTORED_HASH="$(awk -F= '/^vars_file_sha256=/{print $2}' <<<"$RESTORED_OUT")"
[[ "$WRITTEN_HASH" == "$RESTORED_HASH" ]]
[[ "$RESTORED_HASH" == "$(shasum -a 256 "$VARS" | awk '{print $1}')" ]]

if "$BIN" "$FD" --vars-file "$VARS" --expect written >/dev/null 2>&1; then
  echo "FAIL: written mode overwrote an existing vars file" >&2
  exit 1
fi
echo "PASS: separate BridgeVM processes restored one fail-closed vars file"
