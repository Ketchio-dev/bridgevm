#!/usr/bin/env bash
# Policy tests for the Studio live-gate queue.
#
# These assert the properties that make a local queue acceptable on a public
# repository: no inbound listener, no runner registration, atomic job claiming,
# and receipts that are redacted before anyone can read them.
set -euo pipefail

REPO="$(cd "$(dirname "$0")/../.." && pwd)"
CLI="$REPO/scripts/live-gates/bridgevm-live"
WORKER="$REPO/scripts/live-gates/bridgevm-live-worker.sh"
INSTALL="$REPO/scripts/live-gates/install-studio-queue.sh"
PLIST="$REPO/scripts/live-gates/com.ketchio.bridgevm-live.plist"
REDACT="$REPO/scripts/live-gates/redact-receipt.py"
TIER="$REPO/scripts/live-gates/run-tier.sh"
A3_TIER="$REPO/scripts/live-gates/run-a3-title-tier.sh"
A3_RECEIPT="$REPO/scripts/live-gates/write-a3-title-receipt.py"
A3_VERIFY="$REPO/scripts/verify-d3d11-title-fps.sh"
A3_PAYLOAD="$REPO/scripts/live-gates/a3-title-payload.sh"
A3_PAYLOAD_VALIDATOR="$REPO/scripts/live-gates/a3-title-payload.py"
A3_STAGE="$REPO/scripts/live-gates/a3-title-payload-stage.sh"
BOOT_RUNNER="$REPO/scripts/run-hvf-windows-installed-boot-runner.sh"
POSTMORTEM_HARVEST="$REPO/scripts/harvest-hvf-windows-postmortem.sh"

WORK="$(mktemp -d)"
POSTMORTEM_MOUNT=""
cleanup() {
    [[ -z "$POSTMORTEM_MOUNT" ]] || hdiutil detach "$POSTMORTEM_MOUNT" -quiet >/dev/null 2>&1 || true
    rm -rf "$WORK"
}
trap cleanup EXIT
export BRIDGEVM_LIVE_ROOT="$WORK/queue"

checks=0
check() {
    checks=$((checks + 1))
    if ! eval "$2"; then
        echo "FAIL: $1" >&2
        exit 1
    fi
}

# Assert a pattern is absent. Separate from `check` because passing a regex
# through `eval` needs two rounds of quoting and silently mangles backslashes.
# Comments are stripped first: these files *describe* the listener they must
# not open, and matching prose would make the check unfalsifiable in reverse.
no_match() {
    local description="$1" pattern="$2"
    shift 2
    checks=$((checks + 1))
    local hits
    hits="$(cat "$@" | sed 's/#.*$//' | grep -inE "$pattern" || true)"
    if [ -n "$hits" ]; then
        echo "FAIL: $description" >&2
        printf '%s\n' "$hits" >&2
        exit 1
    fi
}

# --- shape ---------------------------------------------------------------
check "the CLI is executable" '[ -x "$CLI" ]'
check "the worker is executable" '[ -x "$WORKER" ]'
check "the installer is executable" '[ -x "$INSTALL" ]'
check "the redactor is executable" '[ -x "$REDACT" ]'
check "the tier dispatcher is executable" '[ -x "$TIER" ]'
check "the A3 tier helper is executable" '[ -x "$A3_TIER" ]'
check "the A3 receipt writer is executable" '[ -x "$A3_RECEIPT" ]'
check "the A3 payload validator is executable" '[ -x "$A3_PAYLOAD" ] && [ -x "$A3_PAYLOAD_VALIDATOR" ]'
check "the A3 payload staging policy is executable" '[ -x "$A3_STAGE" ]'
check "the plist is well formed" 'plutil -lint "$PLIST" >/dev/null'
check "the Windows post-mortem harvester is executable" '[ -x "$POSTMORTEM_HARVEST" ]'
check "the installed-boot runner attaches post-mortem media read-only" \
    'awk '\''/^harvest_guest_windows_postmortem\(\)/,/^}/'\'' "$BOOT_RUNNER" | grep -Fq -- '\''-imagekey diskimage-class=CRawDiskImage -readonly "$TARGET"'\'''
check "the installed-boot runner captures both post-mortem phases" \
    'grep -Fq '\''harvest_guest_windows_postmortem pre-run'\'' "$BOOT_RUNNER" && grep -Fq '\''harvest_guest_windows_postmortem post-run'\'' "$BOOT_RUNNER"'

# --- read-only Windows post-mortem boundary ------------------------------
postmortem_src="$WORK/postmortem-source"
postmortem_image="$WORK/postmortem.dmg"
postmortem_evidence="$WORK/postmortem-evidence"
postmortem_logs="$postmortem_src/Windows/System32/winevt/Logs"
mkdir -p "$postmortem_src/Windows/System32/Tasks" "$postmortem_logs" "$WORK/postmortem-mount"
printf 'agent-current\n' > "$postmortem_src/bvagent.log"
printf '<Task>fixed</Task>\n' > "$postmortem_src/Windows/System32/Tasks/BridgeVM Guest Agent"
printf 'winlogon\n' > "$postmortem_logs/Microsoft-Windows-Winlogon%4Operational.evtx"
printf 'system\n' > "$postmortem_logs/System.evtx"
ln -s System.evtx "$postmortem_logs/Microsoft-Windows-TaskScheduler%4Operational.evtx"
mkfile -n 33554433 "$postmortem_logs/Security.evtx"
printf 'not-allowlisted\n' > "$postmortem_logs/Unexpected.evtx"

check "the post-mortem harvester refuses a writable volume" \
    '! "$POSTMORTEM_HARVEST" --volume "$postmortem_src" --evidence-dir "$WORK/refused" --phase pre-run >/dev/null 2>&1'
hdiutil create -quiet -size 64m -fs HFS+ -srcfolder "$postmortem_src" \
    -format UDRW "$postmortem_image"
postmortem_image_before="$(shasum -a 256 "$postmortem_image" | awk '{print $1}')"
POSTMORTEM_MOUNT="$WORK/postmortem-mount"
hdiutil attach -readonly -nobrowse -mountpoint "$POSTMORTEM_MOUNT" "$postmortem_image" >/dev/null || { echo "FAIL: hdiutil attach" >&2; exit 1; }

"$POSTMORTEM_HARVEST" --volume "$POSTMORTEM_MOUNT" \
    --evidence-dir "$postmortem_evidence" --phase pre-run
check "pre-run post-mortem state is metadata only" \
    'grep -q $'"'"'^agent_log\\t.*\\tmetadata-only\\t'"'"' "$postmortem_evidence/windows-postmortem-pre-run.tsv"'
check "the post-mortem manifest proves a read-only mount" \
    'grep -q $'"'"'^media_read_only\\ttrue$'"'"' "$postmortem_evidence/windows-postmortem-pre-run.tsv" && grep -q $'"'"'^volume_read_only\\ttrue$'"'"' "$postmortem_evidence/windows-postmortem-pre-run.tsv"'
check "pre-run post-mortem capture copies no guest file" \
    '[ ! -e "$postmortem_evidence/windows-postmortem" ]'

"$POSTMORTEM_HARVEST" --volume "$POSTMORTEM_MOUNT" \
    --evidence-dir "$postmortem_evidence" --phase post-run
postmortem_manifest="$postmortem_evidence/windows-postmortem-post-run.tsv"
check "the post-mortem manifest has exactly six fixed artifact ids" \
    '[ "$(awk -F '\''\\t'\'' '\''$1 ~ /^(agent_log|agent_task|winlogon_evtx|task_scheduler_evtx|security_evtx|system_evtx)$/ {n++} END {print n+0}'\'' "$postmortem_manifest")" -eq 6 ]'
check "the post-mortem harvester refuses symlink inputs" \
    'grep -q $'"'"'^task_scheduler_evtx\\t.*\\trefused-symlink\\t'"'"' "$postmortem_manifest"'
check "the post-mortem harvester refuses oversize inputs" \
    'grep -q $'"'"'^security_evtx\\t.*\\trefused-oversize\\t33554433\\t'"'"' "$postmortem_manifest"'
check "the post-mortem harvester copies only bounded allowlisted files" \
    '[ "$(find "$postmortem_evidence/windows-postmortem" -type f | wc -l | tr -d " ")" -eq 4 ]'
check "an unlisted guest event log is never copied" \
    '[ ! -e "$postmortem_evidence/windows-postmortem/Unexpected.evtx" ] && ! grep -q "Unexpected.evtx" "$postmortem_manifest"'
check "post-mortem evidence paths are relative and private-path free" \
    '! grep -E '\''/Users/|/tmp/|BridgeVM/work'\'' "$postmortem_manifest"'

hdiutil detach "$POSTMORTEM_MOUNT" -quiet
POSTMORTEM_MOUNT=""
postmortem_image_after="$(shasum -a 256 "$postmortem_image" | awk '{print $1}')"
check "read-only post-mortem harvest leaves the guest image byte-identical" \
    '[ "$postmortem_image_before" = "$postmortem_image_after" ]'

# --- no inbound network path --------------------------------------------
# A local queue that grew a listener would be a self-hosted runner with extra
# steps, which is exactly what a public repo must not have.
check "the plist declares no socket" '! grep -q "<key>Sockets</key>" "$PLIST"'
check "the plist is not a root daemon" '! grep -qi "UserName.*root" "$PLIST"'
no_match "no component opens a listening socket" \
    'nc +-l|socat|LISTEN|bind\(' "$CLI" "$WORKER" "$TIER" "$A3_TIER" "$A3_RECEIPT" "$A3_PAYLOAD" "$A3_PAYLOAD_VALIDATOR" "$A3_STAGE"
# The installer names actions-runner only to refuse installing beside one, so
# this looks for the act of registering rather than the word.
no_match "nothing registers a GitHub runner" \
    'config\.sh +--url|RUNNER_TOKEN|--runnergroup' "$CLI" "$WORKER" "$INSTALL" "$TIER" "$A3_TIER" "$A3_RECEIPT" "$A3_PAYLOAD" "$A3_PAYLOAD_VALIDATOR" "$A3_STAGE"
check "the installer refuses to sit beside a runner" \
    'grep -q "actions-runner" "$INSTALL"'
no_match "nothing in the queue path uses sudo" \
    '^[^#]*\bsudo\b' "$CLI" "$WORKER" "$INSTALL" "$TIER" "$A3_TIER" "$A3_RECEIPT" "$A3_PAYLOAD" "$A3_PAYLOAD_VALIDATOR" "$A3_STAGE"
check "live tier receipt, clone, stress and diagnostic policies pass" \
    'python3 "$A3_RECEIPT" --self-test | grep -q "PASS" && "$REPO/tests/integration/windows-closure-live-tier-smoke.sh" | grep -q "PASS" && "$REPO/tests/integration/qmp-stress-live-tier-smoke.sh" | grep -q "PASS" && "$REPO/tests/integration/glyph-scene-pilot-smoke.sh" | grep -q "PASS"'
check "the A3 payload archive is fail-closed" \
    '"$A3_PAYLOAD" --self-test | grep -q "PASS"'
check "the A3 payload uses bounded share chunks" \
    '"$A3_STAGE" --self-test | grep -q "PASS"'
check "the A3 payload command survives cmd.exe without literal single quotes" \
    '! grep -Fq -- "-ArchivePrefix '\''" "$A3_STAGE" && ! grep -Fq -- "-ExpectedPayloadSha256 '\''" "$A3_STAGE"'
check "the A3 D3D11 prep survives cmd.exe without an assignable PowerShell path" \
    '! grep -Fq '\''$d='\'' "$A3_VERIFY" && grep -Fq '\''Test-Path -LiteralPath C:\BridgeVM\a2-title\ppsspp\PPSSPPWindowsARM64.exe'\'' "$A3_VERIFY"'
check "the A3 payload is reconstructed and hashed in the guest" \
    'grep -q "ExpectedPayloadSha256" "$REPO/scripts/win-assets/bvgpu-stage-ppsspp.ps1" && grep -q "ExpectedExecutableSha256" "$REPO/scripts/win-assets/bvgpu-stage-ppsspp.ps1"'
check "the A3 verifier installs the sealed payload every run" \
    'grep -q '\''install_a3_payload_guest "$PAYLOAD_SHA" "$EXPECTED_PPSSPP_SHA"'\'' "$A3_VERIFY"'
check "the A3 campaign stops once three of three is impossible" \
    'grep -Fq '\''verify-d3d11-title-fps.sh" || break'\'' "$A3_TIER"'
check "the A3 verifier kills a parked boot after diagnostics" \
    'grep -q "BRIDGEVM_BOOT_PROGRESS_KILL=1" "$A3_VERIFY"'
check "the A3 verifier measures the product shipping renderer path" \
    'grep -q -- "--performance-risk aggressive --virtio-gpu-3d" "$A3_VERIFY"'
check "the A3 share cap remains eight MiB with seven MiB payload parts" \
    'grep -q '\''A3_PAYLOAD_CHUNK_BYTES=$((7 \* 1024 \* 1024))'\'' "$A3_STAGE" && grep -q -- '\''--agent-share-max-kb 8192'\'' "$A3_VERIFY"'
check "the A3 outer wait starts after bounded host preflight" \
    'grep -q "wait_for .*Boot watchdog:.*HOST_PREFLIGHT_TIMEOUT" "$A3_VERIFY"'
check "the A3 outer wait leaves post-mortem diagnostic grace" \
    'grep -q '\''BOOT_TIMEOUT + DIAGNOSTIC_GRACE'\'' "$A3_VERIFY"'
check "the A3 verifier notices an exited launcher while waiting" \
    'grep -q '\''kill -0 "$LAUNCHER"'\'' "$A3_VERIFY"'
check "the A3 verifier requests final diagnostics only from failure cleanup" \
    'grep -q '\''(( status == 0 )) || diagnostic_stop'\'' "$A3_VERIFY" && grep -q '\''mv "$tmp" "$DIAGNOSTIC_STOP_REQUEST"'\'' "$A3_VERIFY"'
check "the A3 diagnostic stop remains bounded by its existing grace" \
    'grep -q '\''deadline=$((SECONDS + DIAGNOSTIC_GRACE))'\'' "$A3_VERIFY"'
check "the installed boot runner explicitly forwards the diagnostic request" \
    'grep -q '\''BRIDGEVM_HOST_DIAGNOSTIC_STOP_REQUEST='\'' "$BOOT_RUNNER"'

# --- submit returns immediately -----------------------------------------
start=$(date +%s)
job_id="$("$CLI" submit t1-vtimer)"
elapsed=$(( $(date +%s) - start ))
check "submit returns a job id" '[ -n "$job_id" ]'
check "submit returns in under 10s" '[ "$elapsed" -lt 10 ]'
check "the job is queued" '"$CLI" status | grep -q "queued .*$job_id"'

check "the job records its commit and tier" 'grep -q "^commit=[0-9a-f]\{40\}$" "$BRIDGEVM_LIVE_ROOT/queued/$job_id/job.env" && grep -q "^tier=t1-vtimer$" "$BRIDGEVM_LIVE_ROOT/queued/$job_id/job.env"'
check "B4, audio and glyph tiers keep fixed scopes" 'audio_job="$($CLI submit t9-audio-teardown)"; glyph_job="$($CLI submit t11-glyph-scene-pilot)"; grep -q "^tier=t9-audio-teardown$" "$BRIDGEVM_LIVE_ROOT/queued/$audio_job/job.env" && grep -q "^tier=t11-glyph-scene-pilot$" "$BRIDGEVM_LIVE_ROOT/queued/$glyph_job/job.env" && grep -q "N=20 OUT=" "$REPO/scripts/live-gates/run-pointer-reliability-tier.sh" && grep -q "N=10" "$REPO/scripts/live-gates/run-audio-teardown-tier.sh" && python3 "$REPO/scripts/live-gates/seal-immutable-audio-sources.py" --self-test | grep -q PASS && python3 "$REPO/scripts/live-gates/write-audio-teardown-receipt.py" --self-test | grep -q PASS; rm -rf "$BRIDGEVM_LIVE_ROOT/queued/$audio_job" "$BRIDGEVM_LIVE_ROOT/queued/$glyph_job"'

# The A3 tier must seal a copied input manifest at submit time. It may not
# retain a caller-owned path that can be edited after submission.
manifest="$WORK/a3-inputs.tsv"
printf 'image\t/tmp/image\t%s\n' "$(printf image | shasum -a 256 | cut -d' ' -f1)" > "$manifest"
printf '#!/bin/sh\nexit 0\n' > "$WORK/probe"; chmod +x "$WORK/probe"
printf 'binary\t%s\t%s\n' "$WORK/probe" "$(shasum -a 256 "$WORK/probe" | cut -d' ' -f1)" >> "$manifest"
a3_job="$($CLI submit t6-a3-title --input-manifest "$manifest")"
check "A3 submit copies the input manifest" \
    '[ -f "$BRIDGEVM_LIVE_ROOT/queued/$a3_job/input-manifest.tsv" ]'
check "A3 submit seals the copied manifest hash" \
    'grep -q "^input_manifest_sha256=[0-9a-f]\{64\}$" "$BRIDGEVM_LIVE_ROOT/queued/$a3_job/job.env"'
check "A3 submit copies and seals the release binary" \
    'test -x "$BRIDGEVM_LIVE_ROOT/queued/$a3_job/hvf_gic_boot_probe" && grep -q "^sealed_binary_sha256=[0-9a-f]\{64\}$" "$BRIDGEVM_LIVE_ROOT/queued/$a3_job/job.env"'
check "A3 submit rejects a missing manifest" \
    '! "$CLI" submit t6-a3-title --input-manifest "$WORK/missing" >/dev/null 2>&1'
check "non-A3 tiers reject an input manifest" \
    '! "$CLI" submit t0-check --input-manifest "$manifest" >/dev/null 2>&1'
malformed_out="$WORK/malformed-a3"
check "the A3 tier refuses a malformed manifest before build" \
    '! "$TIER" t6-a3-title --out "$malformed_out" --input-manifest "$manifest" --job-id policy-smoke >/dev/null 2>&1'
check "the A3 refusal leaves a machine-readable receipt" \
    'python3 - "$malformed_out/receipt.json" <<'"'"'PY'"'"'
import json, sys
r = json.load(open(sys.argv[1]))
required = {"gate_id", "criterion", "tested_commit", "host_os", "host_hardware", "sample_count", "passes", "failures", "evidence_paths"}
assert required <= r.keys()
assert r["criterion"] == "A3" and r["pass"] is False
PY'

# A syntactically complete and correctly hashed manifest must still fail closed
# when its PPSSPP ZIP is unsafe. This reaches the extracted manifest verifier
# and payload validator without invoking codesign or a VM.
sealed="$WORK/sealed-a3"; mkdir -p "$sealed/viogpu"
for key in image vars title d3d11 dxgi virglrenderer moltenvk; do printf '%s' "$key" > "$sealed/$key"; done
printf driver > "$sealed/viogpu/file"
python3 - "$sealed/ppsspp" <<'PY'
import sys, zipfile
with zipfile.ZipFile(sys.argv[1], "w") as payload:
    payload.writestr("../escape", b"bad")
    payload.writestr("ppsspp/PPSSPPWindowsARM64.exe", b"arm64")
PY
complete_manifest="$sealed/manifest.tsv"
for key in image vars title ppsspp d3d11 dxgi virglrenderer moltenvk; do
    printf '%s\t%s\t%s\n' "$key" "$sealed/$key" "$(shasum -a 256 "$sealed/$key" | cut -d' ' -f1)" >> "$complete_manifest"
done
viogpu_hash="$(cd "$sealed/viogpu" && find . -type f -exec shasum -a 256 {} + | LC_ALL=C sort -k2 | shasum -a 256 | cut -d' ' -f1)"
printf 'viogpu_dir\t%s\t%s\n' "$sealed/viogpu" "$viogpu_hash" >> "$complete_manifest"
printf 'binary\t%s\t%s\n' "$WORK/probe" "$(shasum -a 256 "$WORK/probe" | cut -d' ' -f1)" >> "$complete_manifest"
unsafe_out="$WORK/unsafe-a3"
check "the A3 tier refuses an unsafe sealed payload after manifest verification" \
    '! "$A3_TIER" --out "$unsafe_out" --input-manifest "$complete_manifest" --sealed-binary "$WORK/probe" --job-id unsafe-policy >/dev/null 2>&1 && grep -q "refused-ppsspp-payload" "$unsafe_out/receipt.json"'
check "the installed-boot runner honors a sealed prebuilt binary" \
    'grep -Fq '"'"'BRIDGEVM_PREBUILT_PROBE requires an absolute regular release binary with --skip-build'"'"' "$BOOT_RUNNER"'
rm -rf "$BRIDGEVM_LIVE_ROOT/queued/$a3_job"

# --- claiming is atomic --------------------------------------------------
claimed="$("$CLI" next)"
check "next claims the job" '[ -n "$claimed" ]'
check "the claimed job moved to running" '"$CLI" status | grep -q "running .*$job_id"'
check "a second worker cannot claim it" '! "$CLI" next'

# --- receipts are only served redacted -----------------------------------
cat > "$claimed/receipt.json" <<'JSON'
{"gate_id":"a3-d3d11-real-title-3run","criterion":"A3","tested_commit":"0123456789abcdef0123456789abcdef01234567","tier":"t6-a3-title","pass":true,"passes":3,"sample_count":1200,"fps_p50":[58.82,58.82,58.82],"title_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","evidence_paths":["run-1/summary.txt"],"disk_path":"/Users/me/win11.qcow2","vars_path":"/tmp/VARS.fd"}
JSON
check "a raw receipt is never served" '! "$CLI" receipt "$job_id" >/dev/null 2>&1'
python3 "$REDACT" --in "$claimed/receipt.json" --out "$claimed/receipt.public.json"
check "the published receipt is served" '"$CLI" receipt "$job_id" >/dev/null'
check "the published receipt drops the disk path" \
    '! "$CLI" receipt "$job_id" | grep -q "qcow2"'
check "the published receipt drops the vars path" \
    '! "$CLI" receipt "$job_id" | grep -q "VARS.fd"'
check "the published receipt keeps the result" \
    '"$CLI" receipt "$job_id" | grep -q "\"pass\": true"'
check "the published receipt keeps A3 provenance" \
    '"$CLI" receipt "$job_id" | grep -q "\"criterion\": \"A3\""'
check "the published receipt keeps FPS samples" \
    '"$CLI" receipt "$job_id" | grep -q "\"sample_count\": 1200"'
check "the published receipt keeps only relative evidence paths" \
    '"$CLI" receipt "$job_id" | grep -q "run-1/summary.txt"'

# --- cancellation --------------------------------------------------------
check "cancelling a running job requests, not kills" \
    '"$CLI" cancel "$job_id" | grep -q "cancellation requested"'
check "the cancel request is visible to the worker" '[ -f "$claimed/cancel.requested" ]'

second="$("$CLI" submit t0-check)"
check "a queued job cancels immediately" '"$CLI" cancel "$second" | grep -q "canceled"'
check "the canceled job is done" '"$CLI" status | grep -q "done .*$second"'

# --- tiers refuse to invent evidence -------------------------------------
check "an unknown tier is rejected" '! "$TIER" nonsense --out "$WORK/x" 2>/dev/null'
t5_output="$(BASE_IMAGE="$WORK/absent.raw" BASE_VARS="$WORK/absent.fd" \
    INJECTOR="$WORK/absent-inj.raw" \
    "$TIER" t5-campaign --out "$WORK/t5" 2>&1 || true)"
check "boot tiers refuse when the media cannot be read" \
    'printf "%s" "$t5_output" | grep -q "cannot read required Windows media"'
check "the refusal names which input is missing" \
    'printf "%s" "$t5_output" | grep -q "absent-inj.raw"'
check "the refusal is recorded in the receipt" \
    'grep -q "refused-no-media" "$WORK/t5/receipt.json"'

# An unreadable file, not just an absent one: this is the TCC failure mode,
# where the path exists and stat() succeeds but the bytes cannot be read.
: > "$WORK/unreadable.raw"; chmod 000 "$WORK/unreadable.raw"
unreadable_output="$(BASE_IMAGE="$WORK/unreadable.raw" \
    "$TIER" t5-campaign --out "$WORK/t5b" 2>&1 || true)"
chmod 644 "$WORK/unreadable.raw"
check "a present but unreadable input is refused too" \
    'printf "%s" "$unreadable_output" | grep -q "unreadable.raw"'

# --- the installer is safe to inspect ------------------------------------
check "the installer supports a dry run" '"$INSTALL" --dry-run >/dev/null 2>&1 || true'
no_match "the installer stores no credentials" \
    'password|token=|api[_-]key' "$INSTALL"

echo "PASS: live gate policy smoke ($checks checks)"
