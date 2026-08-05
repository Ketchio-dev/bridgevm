#!/usr/bin/env bash
# Dispatch one live-gate tier and leave a receipt in --out.
#
# Tiers (PLAN.md): T1 is the seconds-long vtimer microprobe, T2 a single
# prepared-cache pilot boot, T3 a 3-boot candidate gate, T4 a nightly reset
# soak, T5 the full A1 10-boot campaign. Only T5 produces shipping evidence;
# no lower tier may be used to lower an A1 threshold.
set -euo pipefail

REPO="$(cd "$(dirname "$0")/../.." && pwd)"
TIER="${1:?run-tier.sh needs a tier}"
shift || true

OUT=""
JOB_ID="local-$(date +%Y%m%d-%H%M%S)"
while [ $# -gt 0 ]; do
    case "$1" in
        --out) OUT="$2"; shift 2 ;;
        --job-id) JOB_ID="$2"; shift 2 ;;
        --lanes) LANES="$2"; shift 2 ;;
        *) echo "unknown run-tier option $1" >&2; exit 2 ;;
    esac
done
[ -n "$OUT" ] || { echo "run-tier.sh needs --out" >&2; exit 2; }
mkdir -p "$OUT"

receipt() {
    # Fields here must be on the redact-receipt allowlist or they are dropped.
    cat > "$OUT/receipt.json" <<EOF
{
  "tier": "$TIER",
  "job_id": "$JOB_ID",
  "commit": "$(git -C "$REPO" rev-parse HEAD)",
  "host_model": "$(sysctl -n hw.model)",
  "macos_version": "$(sw_vers -productVersion)",
  "finished_at": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "outcome": "$1",
  "pass": $2
}
EOF
}

case "$TIER" in
    t1-vtimer)
        # Seconds, not minutes: the bare-metal cancellation/vtimer probe.
        if "$REPO/scripts/run-hvf-vtimer-cancel-gate.sh" --out "$OUT"; then
            receipt completed true
        else
            receipt failed false
            exit 1
        fi
        ;;
    t0-check)
        # The deterministic project check, useful as a queue smoke test.
        if "$REPO/scripts/check-project.sh" > "$OUT/check.log" 2>&1; then
            receipt completed true
        else
            receipt failed false
            exit 1
        fi
        ;;
    t1-snapshot)
        # Needs only the internal-volume canonical pair, so it runs even where
        # TCC blocks the external volume.
        echo "running the powered-off snapshot pair gate" >&2
        if OUT="$OUT" "$REPO/scripts/verify-powered-off-snapshot.sh"; then
            receipt completed true
        else
            receipt failed false
            exit 1
        fi
        ;;
    t2-pilot|t3-candidate|t4-soak|t5-campaign)
        # These need private Windows media and 20+ minutes per boot. They are
        # declared so the queue and its policy tests are exercised, but they
        # refuse to invent evidence when the media is absent.
        # Check the four inputs the gate actually reads, not a directory that
        # happens to exist. A mounted volume is not evidence that the files
        # under it are readable -- TCC denies a LaunchAgent reads under
        # /Volumes/* while stat() still succeeds, so the old check passed and
        # the gate then failed minutes later blaming the injector.
        missing=""
        for input in \
            "${BASE_IMAGE:-$HOME/BridgeVM/work/wall-c8-clean-12041.raw}" \
            "${BASE_VARS:-$HOME/BridgeVM/work/wall-c8-clean-inject-vars.fd}" \
            "${INJECTOR:-$HOME/BridgeVM/injectors/inj-a1-20260802.raw}"
        do
            # head -c1 rather than -r: readable-by-policy is what matters here.
            head -c1 "$input" >/dev/null 2>&1 || missing="$missing $input"
        done
        if [ -n "$missing" ]; then
            echo "cannot read required Windows media:$missing" >&2
            echo "if these are on an external volume, macOS TCC is the likely" >&2
            echo "cause; keep them on the internal volume instead" >&2
            receipt refused-no-media false
            exit 1
        fi
        # Boots per tier. T2 proves the prepared cache and the harness; only
        # T5 produces shipping A1 evidence, and its count is the 10 the
        # criterion names. A cheaper tier must never be read as a campaign.
        case "$TIER" in
            t2-pilot)     boots=2  ;;
            t3-candidate) boots=3  ;;
            t4-soak)      boots=5  ;;
            t5-campaign)  boots=10 ;;
        esac
        echo "$TIER requires the boot gate; invoking p1-boot-gate.sh --boots $boots" >&2
        if "$REPO/scripts/p1-boot-gate.sh" --out "$OUT" --boots "$boots"; then
            receipt completed true
        else
            receipt failed false
            exit 1
        fi
        ;;
    *)
        echo "unknown tier $TIER" >&2
        exit 2
        ;;
esac
