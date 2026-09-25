#!/usr/bin/env bash
set -euo pipefail
[[ $# -eq 5 ]] || exit 2
HERE="$(cd "$(dirname "$0")" && pwd)"
case "$1" in
  t1-restore-boot) runner=run-snapshot-restore-tier.py ;;
  t20-a19-native-snapshot-restore) runner=run-native-snapshot-restore-tier.py ;;
  t21-a19-quota-refusal) runner=run-a19-quota-refusal-tier.py ;; t22-a19-interrupted-restore) runner=run-a19-interrupted-restore-tier.py ;;
  *) echo "unknown A19 tier" >&2; exit 2 ;;
esac
exec python3 "$HERE/$runner" "$2" "$3" "$4" "$5"
