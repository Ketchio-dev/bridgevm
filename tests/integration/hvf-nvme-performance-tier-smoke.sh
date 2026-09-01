#!/usr/bin/env bash
# Deterministic T16 contracts only; never boots a VM or measures host storage.
set -euo pipefail
REPO="$(cd "$(dirname "$0")/../.." && pwd -P)"
RENDER="$REPO/scripts/render-hvf-nvme-workload.py"
MANIFEST="$REPO/scripts/live-gates/hvf-nvme-performance-manifest.sh"
RUNNER="$REPO/scripts/live-gates/run-hvf-nvme-performance-tier.sh"
SUBMIT="$REPO/scripts/submit-hvf-nvme-performance-campaign.sh"
WRITER="$REPO/scripts/write-hvf-nvme-performance-receipt.py"
REPORT="$REPO/scripts/hvf_nvme_performance_report.py"
TEMPORARY="$(mktemp -d "${TMPDIR:-/tmp}/bridgevm-t16-smoke.XXXXXX")"
trap "rm -rf '$TEMPORARY'" EXIT
seal() { openssl dgst -sha256 -r "$1" | cut -d' ' -f1 | tr -d '\n'; }

python3 "$RENDER" --self-test | grep -q PASS
bash "$MANIFEST" self-test | grep -q PASS
python3 "$WRITER" --self-test | grep -q PASS
python3 "$REPORT" --self-test | grep -q PASS
python3 "$REPO/scripts/live-gates/redact-receipt.py" --self-test | grep -q PASS
head="$(git -C "$REPO" rev-parse HEAD)"
for key in image vars firmware renderer; do printf 'fixture-%s\n' "$key" > "$TEMPORARY/$key"; done
printf 'baseline-binary\n' > "$TEMPORARY/binary-a"; printf 'candidate-binary\n' > "$TEMPORARY/binary-b"
python3 "$RENDER" --output "$TEMPORARY/base-workload.ps1" \
  --config-output "$TEMPORARY/base-config.json" --nonce 0123456789abcdef >/dev/null

base_manifest() {
  local output="$1" binary="$2" key path
  : > "$output"
  for key in image vars firmware renderer; do path="$TEMPORARY/$key"; printf '%s\t%s\t%s\n' "$key" "$path" "$(seal "$path")" >> "$output"; done
  printf 'binary\t%s\t%s\nconfig\t%s\t%s\nworkload_script\t%s\t%s\n' \
    "$binary" "$(seal "$binary")" "$TEMPORARY/base-config.json" "$(seal "$TEMPORARY/base-config.json")" \
    "$TEMPORARY/base-workload.ps1" "$(seal "$TEMPORARY/base-workload.ps1")" >> "$output"
  printf 'workload_profile\twindows-nvme-warm-seq-v1\nfile_mib\t512\ntransfer_kib\t128\nread_passes\t5\nwrite_passes\t2\n' >> "$output"
}
base_manifest "$TEMPORARY/base-a.tsv" "$TEMPORARY/binary-a"
base_manifest "$TEMPORARY/base-b.tsv" "$TEMPORARY/binary-b"
aa="$("$SUBMIT" --mode AA --pairs 10 --baseline-manifest "$TEMPORARY/base-a.tsv" \
  --harness-sha "$head" --queue-root "$TEMPORARY/queue" --validate-only)"
[[ "$(grep -c '^ordinal=' <<< "$aa")" == 20 && "$aa" == *'validation=pass'* ]]
ab="$("$SUBMIT" --mode AB --pairs 10 --baseline-manifest "$TEMPORARY/base-a.tsv" \
  --candidate-manifest "$TEMPORARY/base-b.tsv" --harness-sha "$head" \
  --queue-root "$TEMPORARY/queue" --validate-only)"
[[ "$(grep -c '^ordinal=' <<< "$ab")" == 20 && "$ab" == *'order=AB'* && "$ab" == *'order=BA'* ]]
! "$SUBMIT" --mode AB --pairs 10 --baseline-manifest "$TEMPORARY/base-a.tsv" --candidate-manifest "$TEMPORARY/base-a.tsv" --validate-only >/dev/null 2>&1
! "$SUBMIT" --mode AA --pairs 9223372036854775806 --baseline-manifest "$TEMPORARY/base-a.tsv" --validate-only >/dev/null 2>&1
! BRIDGEVM_LIVE_MIN_FREE_GIB=9223372036854775806 "$SUBMIT" --mode AA --pairs 10 --baseline-manifest "$TEMPORARY/base-a.tsv" --validate-only >/dev/null 2>&1

campaign=00000000000000000000000000000000
printf 'schema\tbridgevm.t16-campaign-registry.v1\ncampaign_id\t%s\ncampaign_mode\tAA\npairs\t10\nexpected_runs\t20\nharness_commit\t%s\n' "$campaign" "$head" > "$TEMPORARY/campaign_registry"
for ordinal in $(seq 1 20); do
  (( ((((ordinal - 1) / 2) + ordinal) % 2) == 1 )) && role=baseline || role=candidate
  printf 'lane\t%s\t%s\tt16-%s-%03d\n' "$ordinal" "$role" "$campaign" "$ordinal" >> "$TEMPORARY/campaign_registry"
done
nonce="$(printf '%s' "$campaign:1" | openssl dgst -sha256 -r | cut -c1-32)"
python3 "$RENDER" --output "$TEMPORARY/lane-workload.ps1" \
  --config-output "$TEMPORARY/lane-config.json" --nonce "$nonce" >/dev/null
lane="$TEMPORARY/lane.tsv"; : > "$lane"
for key in image vars firmware renderer campaign_registry; do printf '%s\t%s\t%s\n' "$key" "$TEMPORARY/$key" "$(seal "$TEMPORARY/$key")" >> "$lane"; done
printf 'binary\t%s\t%s\nconfig\t%s\t%s\nworkload_script\t%s\t%s\n' \
  "$TEMPORARY/binary-a" "$(seal "$TEMPORARY/binary-a")" "$TEMPORARY/lane-config.json" "$(seal "$TEMPORARY/lane-config.json")" \
  "$TEMPORARY/lane-workload.ps1" "$(seal "$TEMPORARY/lane-workload.ps1")" >> "$lane"
printf 'campaign_id\t%s\ncampaign_mode\tAA\ncampaign_role\tbaseline\ncampaign_ordinal\t1\ncampaign_expected_runs\t20\nworkload_profile\twindows-nvme-warm-seq-v1\nfile_mib\t512\ntransfer_kib\t128\nread_passes\t5\nwrite_passes\t2\n' "$campaign" >> "$lane"
"$RUNNER" --out "$TEMPORARY/out" --input-manifest "$lane" \
  --sealed-binary "$TEMPORARY/binary-a" --validate-only | grep -q PASS
printf 'fabricated-workload\n' > "$TEMPORARY/fake.ps1"
awk -F '\t' -v p="$TEMPORARY/fake.ps1" -v h="$(seal "$TEMPORARY/fake.ps1")" \
  'BEGIN{OFS="\t"} $1=="workload_script"{$2=p;$3=h} {print}' "$lane" > "$TEMPORARY/fake.tsv"
! "$RUNNER" --out "$TEMPORARY/bad-out" --input-manifest "$TEMPORARY/fake.tsv" \
  --sealed-binary "$TEMPORARY/binary-a" --validate-only >/dev/null 2>&1
grep -Fq 'BRIDGEVM_LIVE_ROOT="$QUEUE_ROOT" "$CLI" submit t16-hvf-nvme-performance' "$SUBMIT"
grep -Fq -- '--job-id "$expected_job"' "$SUBMIT"
grep -Fq 'campaign-registry.tsv' "$SUBMIT" "$REPORT"
grep -Fq 't16-hvf-nvme-performance' "$REPO/scripts/live-gates/bridgevm-live" "$REPO/scripts/live-gates/bridgevm-live-worker.sh" "$REPO/scripts/live-gates/run-tier.sh"
echo "PASS: sealed T16 Windows warm NVMe performance contracts"
