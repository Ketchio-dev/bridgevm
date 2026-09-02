#!/usr/bin/env bash
# Dispatch one sealed T16 manifest to its exact versioned runner.
set -euo pipefail

REPO="$(cd "$(dirname "$0")/../.." && pwd -P)"
manifest=""
args=("$@")
while [[ $# -gt 0 ]]; do
  case "$1" in
    --input-manifest) [[ $# -ge 2 ]] || exit 2; manifest="$2"; shift 2 ;;
    --out|--sealed-binary|--job-id) [[ $# -ge 2 ]] || exit 2; shift 2 ;;
    --validate-only) shift ;;
    *) printf 'unknown NVMe-performance option %s\n' "$1" >&2; exit 2 ;;
  esac
done
[[ -f "$manifest" && ! -L "$manifest" ]] || { printf '%s\n' 'T16 needs a regular sealed input manifest' >&2; exit 2; }
profile="$(LC_ALL=C awk -F '\t' '$1 == "workload_profile" { print $2; count++ } END { if (count != 1) exit 1 }' "$manifest")" \
  || { printf '%s\n' 'T16 manifest has no unique workload profile' >&2; exit 2; }
case "$profile" in
  windows-nvme-warm-seq-v1) runner=run-hvf-nvme-performance-v1-tier.sh ;;
  windows-nvme-warm-seq-v2) runner=run-hvf-nvme-performance-v2-tier.sh ;;
  *) printf 'unsupported T16 workload profile: %s\n' "$profile" >&2; exit 2 ;;
esac
exec "$REPO/scripts/live-gates/$runner" "${args[@]}"
