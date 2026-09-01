#!/usr/bin/env bash

# Detach a job-owned disk image without allowing hdiutil to stall a gate.
bridgevm_detach_image() {
  local device="$1" deadline pid rc=0
  [[ "$device" == /dev/* ]] || return 2
  deadline=$((SECONDS + 30))
  hdiutil detach "$device" -quiet &
  pid=$!
  while kill -0 "$pid" 2>/dev/null; do
    if (( SECONDS >= deadline )); then
      kill -TERM "$pid" 2>/dev/null || true
      wait "$pid" 2>/dev/null || true
      return 124
    fi
    sleep 0.1
  done
  wait "$pid" || rc=$?
  (( rc == 0 )) || return "$rc"
  if hdiutil info 2>/dev/null | awk -v device="$device" '$1 == device { found=1 } END { exit !found }'; then
    return 1
  fi
}
