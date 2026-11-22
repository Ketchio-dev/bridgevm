#!/usr/bin/env bash
# Dispatch one physical-Mac live-gate tier and leave a receipt in --out.
#
set -euo pipefail
REPO="$(cd "$(dirname "$0")/../.." && pwd)"
TIER="${1:?run-tier.sh needs a tier}"
[[ "$TIER" != t0-check ]] || { echo "deterministic checks belong on GitHub-hosted Actions, not the physical-Mac live queue" >&2; exit 2; }
shift || true

OUT=""
INPUT_MANIFEST=""
SEALED_BINARY=""
JOB_ID="local-$(date +%Y%m%d-%H%M%S)"
while [ $# -gt 0 ]; do
    case "$1" in
        --out) OUT="$2"; shift 2 ;;
        --job-id) JOB_ID="$2"; shift 2 ;;
        --input-manifest) INPUT_MANIFEST="$2"; shift 2 ;;
        --sealed-binary) SEALED_BINARY="$2"; shift 2 ;;
        --lanes) LANES="$2"; shift 2 ;;
        *) echo "unknown run-tier option $1" >&2; exit 2 ;;
    esac
done
[ -n "$OUT" ] || { echo "run-tier.sh needs --out" >&2; exit 2; }
mkdir -p "$OUT"

# Hash of an input the tier will read, or "absent". Sealing these is what
# makes a receipt reproducible: the images live outside the repository and
# are mutable in principle. openssl, not shasum: shasum is Perl and ~5x
# slower, which on a 64 GiB image is minutes.
seal() {
    [[ -r "$1" ]] || { printf 'absent'; return; }
    openssl dgst -sha256 -r "$1" 2>/dev/null | cut -d' ' -f1 | tr -d '\n'
}

receipt() {
    # Fields here must be on the redact-receipt allowlist or they are dropped.
    cat > "$OUT/receipt.json" <<EOF
{
  "tier": "$TIER",
  "job_id": "$JOB_ID",
  "commit": "$(git -C "$REPO" rev-parse HEAD)",
  "image_sha256": "$(seal "${SEALED_IMAGE:-}")",
  "vars_sha256": "$(seal "${SEALED_VARS:-}")",
  "host_model": "$(sysctl -n hw.model)",
  "macos_version": "$(sw_vers -productVersion)",
  "finished_at": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "outcome": "$1",
  "pass": $2
}
EOF
}

case "$TIER" in
    t1-snapshot)
        # Needs only the internal-volume canonical pair, so it runs even where
        # TCC blocks the external volume.
        SEALED_IMAGE=$HOME/BridgeVM/work/canonical-fresh-12041-agent-20260730.raw
        SEALED_VARS=$HOME/BridgeVM/work/canonical-fresh-12041-agent-20260730-vars.fd
        echo "running the powered-off snapshot pair gate" >&2
        if OUT="$OUT" "$REPO/scripts/verify-powered-off-snapshot.sh"; then
            receipt completed true
        else
            receipt failed false
            exit 1
        fi
        ;;
    t1-restore-boot)
        exec python3 "$REPO/scripts/live-gates/run-snapshot-restore-tier.py" \
            "$OUT" "$JOB_ID" "$INPUT_MANIFEST" "$SEALED_BINARY"
        ;;
    t17-windows-hvf-product-e2e|t18-audio-teardown|t19-windows-hvf-import-product-e2e|d5-guest-input|d6-app-ui|d6-app-ui-host-v1|d6-app-ui-host-v2|d6-app-ui-install-cancel-v1)
        "$REPO/scripts/live-gates/run-special-tier.sh" \
          "$TIER" "$OUT" "$JOB_ID" "$INPUT_MANIFEST" "$SEALED_BINARY" ;;
    *)
        echo "unknown tier $TIER" >&2
        exit 2
        ;;
esac
