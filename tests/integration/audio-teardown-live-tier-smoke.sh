#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd -P)"
TMP_PARENT="${TMPDIR:-/tmp}"; TMP_PARENT="${TMP_PARENT%/}"
TMP="$(mktemp -d "$TMP_PARENT/bridgevm-b7-audio.XXXXXX")"; trap 'rm -rf "$TMP"' EXIT
MANIFEST_TOOL="$ROOT/scripts/live-gates/audio-teardown-manifest.py"
WRITER="$ROOT/scripts/live-gates/write-audio-teardown-receipt.py"
VERIFY="$ROOT/scripts/verify-audio-teardown-receipt.py"
TIER="$ROOT/scripts/live-gates/run-audio-teardown-tier.sh"
PUBLISH="$ROOT/scripts/live-gates/publish-audio-teardown-receipt.sh"
CLI="$ROOT/scripts/live-gates/bridgevm-live"; SPECIAL="$ROOT/scripts/live-gates/run-special-tier.sh"
GENERIC_PUBLISH="$ROOT/scripts/live-gates/publish-receipt.sh"; MISSING="$ROOT/scripts/live-gates/write-missing-receipt.sh"; RECOVER="$ROOT/scripts/live-gates/recover-stale-receipt.sh"
HEAD="$(git -C "$ROOT" rev-parse HEAD)"; STARTED="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
seal() { shasum -a 256 "$1" | cut -d' ' -f1; }
for key in image vars binary firmware; do printf 'sealed-%s\n' "$key" > "$TMP/$key"; done
SCRIPT="$ROOT/scripts/win-assets/bv-audio-teardown.ps1"; MANIFEST="$TMP/manifest.tsv"
cat > "$MANIFEST" <<EOF
schema	bridgevm.b7-audio-teardown-input.v1
profile	windows-hda-coreaudio-playback-shutdown-v1
sample_count	10
ram_mib	6144
smp_cpus	4
sample_rate_hz	48000
tone_hz	440
tone_seconds	2
image	$TMP/image	$(seal "$TMP/image")
vars	$TMP/vars	$(seal "$TMP/vars")
binary	$TMP/binary	$(seal "$TMP/binary")
firmware	$TMP/firmware	$(seal "$TMP/firmware")
playback_script	scripts/win-assets/bv-audio-teardown.ps1	$(seal "$SCRIPT")
EOF
job="$(BRIDGEVM_LIVE_ROOT="$TMP/queue" "$CLI" submit t18-audio-teardown --sha "$HEAD" --input-manifest "$MANIFEST" --job-id b7-submit-fixture)"
test "$job" = b7-submit-fixture
test -f "$TMP/queue/queued/$job/input-manifest.tsv" -a -x "$TMP/queue/queued/$job/hvf_gic_boot_probe"
VERIFIED="$TMP/verified.json"
python3 "$MANIFEST_TOOL" --manifest "$MANIFEST" --repo "$ROOT" --sealed-binary "$TMP/binary" --out "$VERIFIED"
PRIVATE="$TMP/private"; mkdir "$PRIVATE"
zero_stats='hda CoreAudio stats: frames_rendered=96000 drops=0 dropped_bytes=0 format_drops=0 ring_full_drops=0 queue_stop_errors=0 queue_dispose_errors=0 callback_errors=0 callback_active_errors=0 callback_stopping_errors=0 callback_expected_stopping_errors=0 callback_unexpected_errors=0 callback_stopping_invalid_run_state=0 callback_stopping_queue_invalidated=0 callback_stopping_enqueue_during_reset=0 callback_stopping_disposal_pending=0 callback_stopping_unclassified=0'
expected_stats="${zero_stats/callback_errors=0/callback_errors=1}"
expected_stats="${expected_stats/callback_stopping_errors=0/callback_stopping_errors=1}"
expected_stats="${expected_stats/callback_expected_stopping_errors=0/callback_expected_stopping_errors=1}"
expected_stats="${expected_stats/callback_stopping_enqueue_during_reset=0/callback_stopping_enqueue_during_reset=1}"
for ordinal in {1..10}; do
  lane="$PRIVATE/lane-$ordinal"; mkdir -p "$lane/share"
  nonce="$(printf 'b7-fixture:%s' "$ordinal" | shasum -a 256 | cut -c1-64)"
  printf '%s\n' "$nonce" > "$lane/nonce"; printf '0\n' > "$lane/launcher.exit"
  printf 'B7 PLAYBACK PASS nonce=%s wav_bytes=384044\n' "$nonce" > "$lane/share/b7-audio-result-${nonce:0:12}.txt"
  if [[ "$ordinal" == 1 ]]; then
    printf '%s\n' 'hda CoreAudio callback enqueue: state=stopping reason=stopping-enqueue-during-reset osstatus=-66632 expected=true' >> "$lane/run.log"
    printf '%s\n' "$expected_stats" >> "$lane/run.log"
  else printf '%s\n' "$zero_stats" >> "$lane/run.log"; fi
  printf '%s\n' 'hda CoreAudio lifecycle: operation=stop osstatus=0 success=true' 'hda CoreAudio lifecycle: operation=dispose osstatus=0 success=true' >> "$lane/run.log"
  printf '%s\n' 'stop: PSCI SYSTEM_OFF (system off)' 'NVMe disk written back: fixture' >> "$lane/run.log"
done
OUT="$TMP/out"; mkdir "$OUT"
python3 "$WRITER" --out "$OUT/receipt.json" --private "$PRIVATE" --manifest "$MANIFEST" \
  --verified "$VERIFIED" --sealed-binary "$TMP/binary" --job-id b7-pass-fixture \
  --commit "$HEAD" --started-at "$STARTED" --attempts 10 --outcome completed \
  --failure-code none --cleanup-verified
python3 "$VERIFY" "$OUT/receipt.json" --expected-commit "$HEAD" >/dev/null
python3 - "$OUT/receipt.json" <<'PY'
import json,sys
r=json.load(open(sys.argv[1])); assert r["pass"] and r["criterion_pass"]
assert (r["run_count"],r["passes"],r["failures"]) == (10,10,0)
assert r["callback_errors_total"] == r["callback_expected_stopping_errors_total"] == 1
assert r["callback_active_errors_total"] == r["callback_unexpected_errors_total"] == 0
assert r["queue_stop_errors_total"] == r["queue_dispose_errors_total"] == 0
PY
"$PUBLISH" "$OUT" "$ROOT" "$HEAD"; cmp -s "$OUT/receipt.json" "$OUT/receipt.public.json"
bad_publish="$TMP/bad-publish"; mkdir "$bad_publish"; cp "$OUT/receipt.json" "$bad_publish/receipt.json"
python3 - "$bad_publish/receipt.json" <<'PY'
import json,sys
p=sys.argv[1]; r=json.load(open(p)); r["callback_active_errors_total"]=1; open(p,"w").write(json.dumps(r)+"\n")
PY
! "$PUBLISH" "$bad_publish" "$ROOT" "$HEAD" >/dev/null 2>&1
test ! -e "$bad_publish/receipt.public.json"
failed_private="$TMP/failed-private"; mkdir "$failed_private"; cp -R "$PRIVATE/lane-1" "$failed_private/lane-1"
python3 - "$failed_private/lane-1/run.log" <<'PY'
import pathlib,sys
p=pathlib.Path(sys.argv[1]); p.write_text(p.read_text().replace("callback_errors=1", "callback_errors=2"))
PY
failed_out="$TMP/failed-out"; mkdir "$failed_out"
python3 "$WRITER" --out "$failed_out/receipt.json" --private "$failed_private" --manifest "$MANIFEST" \
  --verified "$VERIFIED" --sealed-binary "$TMP/binary" --job-id b7-fail-fixture \
  --commit "$HEAD" --started-at "$STARTED" --attempts 1 --outcome failed \
  --failure-code lane-failed --cleanup-verified
python3 "$VERIFY" "$failed_out/receipt.json" --expected-commit "$HEAD" >/dev/null
grep -q '"pass": false' "$failed_out/receipt.json"
validate_out="$TMP/validate-out"; "$TIER" --out "$validate_out" --input-manifest "$MANIFEST" \
  --sealed-binary "$TMP/binary" --job-id b7-validate --validate-only | grep -q PASS
bad_manifest="$TMP/bad.tsv"
python3 - "$MANIFEST" "$bad_manifest" <<'PY'
import pathlib,sys
source=pathlib.Path(sys.argv[1]).read_text().splitlines()
source[8]="image\t/missing-b7-image\t" + "0" * 64
pathlib.Path(sys.argv[2]).write_text("\n".join(source) + "\n")
PY
preflight_out="$TMP/preflight-out"
! "$TIER" --out "$preflight_out" --input-manifest "$bad_manifest" \
  --sealed-binary "$TMP/binary" --job-id b7-preflight >/dev/null 2>&1
python3 "$VERIFY" "$preflight_out/receipt.json" --expected-commit "$HEAD" >/dev/null
grep -q '"outcome": "preflight-blocked"' "$preflight_out/receipt.json"
missing="$TMP/missing"; mkdir -p "$missing/private"; cp "$MANIFEST" "$missing/input-manifest.tsv"; cp "$TMP/binary" "$missing/hvf_gic_boot_probe"
printf 'started_at=%s\n' "$STARTED" > "$missing/job.env"
"$ROOT/scripts/live-gates/write-audio-teardown-missing-receipt.sh" "$missing" "$ROOT" b7-missing "$HEAD"
python3 "$VERIFY" "$missing/receipt.json" --expected-commit "$HEAD" >/dev/null
grep -q '"failure_code": "missing-tier-receipt"' "$missing/receipt.json"
python3 - "$SCRIPT" <<'PY'
import sys
b=open(sys.argv[1],"rb").read(); assert b and b.count(b"\n") == b.count(b"\r\n")
PY
grep -Fq 't18-audio-teardown' "$CLI" "$ROOT/scripts/live-gates/run-tier.sh" "$SPECIAL"
grep -Fq 'publish-audio-teardown-receipt.sh' "$GENERIC_PUBLISH"
grep -Fq 'write-audio-teardown-missing-receipt.sh' "$MISSING"
grep -Fq 't18-audio-teardown' "$RECOVER"
! grep -Eq 'nc +-l|socat|LISTEN|bind\(|sudo|actions-runner' "$TIER" "$ROOT/scripts/live-gates/run-audio-teardown-lane.sh" "$PUBLISH"
grep -Fq 'Invoke-CimMethod -ClassName Win32_Process -MethodName Create' "$ROOT/scripts/live-gates/run-audio-teardown-lane.sh"; grep -Fq 'result_name="b7-audio-result-' "$ROOT/scripts/live-gates/run-audio-teardown-lane.sh"
grep -Fq 'for ordinal in {1..10}' "$TIER"; grep -Fq 'cp -c "$IMAGE"' "$TIER"
echo 'PASS: B7 audio teardown live-tier contract'
