#!/usr/bin/env bash
# Fail closed when a BridgeVM-owned firmware source or image names a compatibility platform.
set -euo pipefail

mode="${1:?reference scan needs tree or stream}"
label="${2:?reference scan needs a label}"
shift 2
pattern='qemu|armvirt|ovmf|fw[_-]?cfg|u[t]m'
set +e
if [[ "$mode" == tree ]]; then
  [[ $# -gt 0 ]] || { echo "reference tree scan needs a path" >&2; exit 2; }
  output="$(LC_ALL=C grep -RInIE "$pattern" -- "$@" 2>&1)"; status=$?
elif [[ "$mode" == stream ]]; then
  [[ $# -eq 0 ]] || { echo "reference stream scan takes no path" >&2; exit 2; }
  output="$(LC_ALL=C grep -inE "$pattern" 2>&1)"; status=$?
else
  echo "unknown reference scan mode: $mode" >&2; exit 2
fi
set -e
case "$status" in
  0) printf '%s\n' "$output" >&2; echo "$label contains a prohibited compatibility-platform reference" >&2; exit 1 ;;
  1) exit 0 ;;
  *) printf '%s\n' "$output" >&2; echo "$label could not be scanned" >&2; exit 2 ;;
esac
