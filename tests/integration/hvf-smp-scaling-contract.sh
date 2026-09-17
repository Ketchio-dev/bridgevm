#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT"
STORE="$(mktemp -d /tmp/bridgevm-smp-contract.XXXXXX)"
trap 'rm -rf "$STORE"' EXIT
fail() { echo "HVF SMP scaling contract: FAIL ($*)" >&2; exit 1; }
seal() { openssl dgst -sha256 -r "$1" | cut -d' ' -f1; }

python3 scripts/hvf_smp_scaling.py self-test >/dev/null
for key in image vars binary renderer; do printf '%s' "$key" > "$STORE/$key"; done
cat > "$STORE/manifest.tsv" <<EOF
image	$STORE/image	$(seal "$STORE/image")
vars	$STORE/vars	$(seal "$STORE/vars")
binary	$STORE/binary	$(seal "$STORE/binary")
renderer	$STORE/renderer	$(seal "$STORE/renderer")
binary_source_commit	$(git rev-parse HEAD)
binary_profile	release
binary_features	venus
rust_toolchain	1.94.0
workload_profile	shipping-core-3d-off-smp-scaling-v1
smp_cpus	4,6,8
runs_per_config	3
EOF
python3 scripts/hvf_smp_scaling.py validate-manifest "$STORE/manifest.tsv" >/dev/null
printf x >> "$STORE/image"
if python3 scripts/hvf_smp_scaling.py validate-manifest "$STORE/manifest.tsv" >/dev/null; then
  fail "mutated image was accepted"
fi
printf image > "$STORE/image"

COPY_MEDIA=0
source scripts/live-gates/hvf_boot_matrix_media.sh
mkdir "$STORE/lane"
prepare_media_file "$STORE/image" "$STORE/lane/image"
prepare_media_file "$STORE/vars" "$STORE/lane/vars"
verify_matrix_media "$STORE/lane" "$STORE/lane/image" "$STORE/lane/vars" 1 4 "$(seal "$STORE/image")" "$(seal "$STORE/vars")"
[[ -f "$STORE/lane/media-integrity.txt" ]] || fail "clone integrity record is missing"
if (verify_matrix_media "$STORE/lane" "$STORE/lane/image" "$STORE/lane/vars" 1 4 "$(printf 0%.0s {1..64})" "$(seal "$STORE/vars")") >/dev/null 2>&1; then
  fail "wrong clone hash was accepted"
fi

queue="$STORE/queue"
job="$(BRIDGEVM_LIVE_ROOT="$queue" scripts/live-gates/bridgevm-live submit d7-hvf-smp-scaling --sha "$(git rev-parse HEAD)" --input-manifest "$STORE/manifest.tsv")"
[[ -f "$queue/queued/$job/input-manifest.tsv" ]] || fail "D7 manifest was not queued"
[[ -f "$queue/queued/$job/hvf_gic_boot_probe" ]] || fail "D7 binary was not sealed into the queue"
grep -q 'd7-hvf-smp-scaling' scripts/live-gates/run-special-tier.sh || fail "D7 runner dispatch is missing"
grep -q 'd7-hvf-smp-scaling' scripts/live-gates/run-tier.sh || fail "D7 top-level dispatch is missing"

echo "HVF SMP scaling contract: PASS"
