#!/usr/bin/env bash
# The caller supplies RUN_LOG and send_ok; only this request's proof may count.
b6_display_matches() {
  local width="$1" height="$2" before reply proof command
  case "${width}x${height}" in 1280x720|1600x900|1920x1080) ;; *) return 1 ;; esac
  before=$(wc -l < "${RUN_LOG:?}") || return 1
  command="powershell -NoProfile -ExecutionPolicy Bypass -File C:\\BridgeVMClosure\\bv-windows-closure-proof.ps1 -Action Display -RequestedMode ${width}x${height}"
  send_ok "$command" || return 1
  reply=$(tail -n "+$((before + 1))" "$RUN_LOG" | tr '\r' '\n') || return 1
  proof=$(printf '%s\n' "$reply" | grep '^BVF2 ') || return 1
  [[ "$proof" != *$'\n'* ]] || return 1
  printf '%s\n' "$proof" | grep -Eq "^BVF2 device=[^[:space:]]+ current=${width}x${height} modes=([2-9]|[1-9][0-9]+) has_${width}x${height}=True$"
}
