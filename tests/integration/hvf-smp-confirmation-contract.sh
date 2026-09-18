#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT"
STORE="$(mktemp -d /tmp/bridgevm-smp-confirmation.XXXXXX)"
trap 'rm -rf "$STORE"' EXIT
fail() { echo "HVF SMP confirmation contract: FAIL ($*)" >&2; exit 1; }
seal() { openssl dgst -sha256 -r "$1" | cut -d' ' -f1; }

python3 scripts/hvf_smp_confirmation.py self-test >/dev/null
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
workload_profile	shipping-core-3d-off-smp-confirmation-v1
smp_cpus	4,6
pairs	12
aa_noise_upper_percent	16.6973
EOF
python3 scripts/hvf_smp_confirmation.py validate-manifest "$STORE/manifest.tsv" >/dev/null
cp "$STORE/manifest.tsv" "$STORE/bad.tsv"
sed -i '' 's/^pairs	12$/pairs	11/' "$STORE/bad.tsv"
if python3 scripts/hvf_smp_confirmation.py validate-manifest "$STORE/bad.tsv" >/dev/null; then
  fail "changed pair count was accepted"
fi

queue="$STORE/queue"
job="$(BRIDGEVM_LIVE_ROOT="$queue" scripts/live-gates/bridgevm-live submit d8-hvf-smp-confirmation --sha "$(git rev-parse HEAD)" --input-manifest "$STORE/manifest.tsv")"
[[ -f "$queue/queued/$job/input-manifest.tsv" ]] || fail "D8 manifest was not queued"
[[ -f "$queue/queued/$job/hvf_gic_boot_probe" ]] || fail "D8 binary was not sealed into the queue"
cmp -s "$STORE/binary" "$queue/queued/$job/hvf_gic_boot_probe" || fail "queued D8 binary changed"
grep -q 'd8-hvf-smp-confirmation' scripts/live-gates/run-special-tier.sh || fail "D8 runner dispatch is missing"
grep -q 'd8-hvf-smp-confirmation' scripts/live-gates/run-tier.sh || fail "D8 top-level dispatch is missing"

echo "HVF SMP confirmation contract: PASS"
