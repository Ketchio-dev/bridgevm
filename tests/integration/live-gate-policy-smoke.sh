#!/usr/bin/env bash
# Synthetic queue policy: no listener/runner, atomic claims, redacted receipts.
set -eEuo pipefail
trap 'printf "FAIL: live gate policy: line=%s status=%s command=%s\n" "$LINENO" "$?" "$BASH_COMMAND" >&2' ERR
REPO="$(cd "$(dirname "$0")/../.." && pwd)"
CLI="$REPO/scripts/live-gates/bridgevm-live"
WORKER="$REPO/scripts/live-gates/bridgevm-live-worker.sh"
RECOVER="$REPO/scripts/live-gates/recover-stale-jobs.sh"
INSTALL="$REPO/scripts/live-gates/install-studio-queue.sh"
PLIST="$REPO/scripts/live-gates/com.ketchio.bridgevm-live.plist"
REDACT="$REPO/scripts/live-gates/redact-receipt.py"; PUBLISH="$REPO/scripts/live-gates/publish-receipt.sh"; FENCE="$REPO/scripts/live-gates/t17-worker-cleanup-fence.sh"
TIER="$REPO/scripts/live-gates/run-tier.sh"; T17_TIER="$REPO/scripts/live-gates/run-windows-product-e2e-tier.sh"
BOOT_RUNNER="$REPO/scripts/run-hvf-windows-installed-boot-runner.sh"
POSTMORTEM_HARVEST="$REPO/scripts/harvest-hvf-windows-postmortem.sh"
WORK="$(mktemp -d "${TMPDIR:-/tmp}/bridgevm-check.XXXXXX")"
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
# Keep regexes out of eval and ignore comments that describe forbidden shapes.
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
check "the CLI is executable" '[ -x "$CLI" ]'
check "the worker is executable" '[ -x "$WORKER" ]'
check "the stale-job reconciler is executable" '[ -x "$RECOVER" ]'
check "the installer is executable" '[ -x "$INSTALL" ]'
check "the redactor is executable" '[ -x "$REDACT" ]'
check "the tier dispatcher is executable" '[ -x "$TIER" ]'
check "the plist is well formed" 'plutil -lint "$PLIST" >/dev/null'
check "the queue stays background while each requested gate gets app scheduling policy" \
    'grep -A1 -q "<key>ProcessType</key>" "$PLIST" && grep -Fq '"'"'/usr/sbin/taskpolicy -a /usr/bin/caffeinate'"'"' "$WORKER"'
check "the clean worker uses Command Line Tools without accepting an Xcode license" 'grep -Fq '"'"'export DEVELOPER_DIR=/Library/Developer/CommandLineTools'"'"' "$WORKER" && grep -Fq '"'"'DEVELOPER_DIR="${DEVELOPER_DIR:-}"'"'"' "$WORKER"'
check "the worker resolves only pushed exact commits and refuses unresolved worktrees" \
    'grep -Fq '"'"'fetch --no-tags origin "$commit"'"'"' "$WORKER" && grep -Fq '"'"'[[ ! "$commit" =~ ^[0-9a-f]{40}$ ]]'"'"' "$WORKER" && grep -Fq '"'"'cat-file -e "$commit^{commit}"'"'"' "$WORKER" && grep -q '"'"'refused-unknown-commit\|refused-worktree'"'"' "$WORKER"'
check "the Windows post-mortem harvester is executable" '[ -x "$POSTMORTEM_HARVEST" ]'
check "the installed-boot runner attaches post-mortem media read-only" \
    'awk '\''/^harvest_guest_windows_postmortem\(\)/,/^}/'\'' "$BOOT_RUNNER" | grep -Fq -- '\''-imagekey diskimage-class=CRawDiskImage -readonly "$TARGET"'\'''
check "the installed-boot runner captures both post-mortem phases" \
    'grep -Fq '\''harvest_guest_windows_postmortem pre-run'\'' "$BOOT_RUNNER" && grep -Fq '\''harvest_guest_windows_postmortem post-run'\'' "$BOOT_RUNNER"'

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

# A listener would turn this local queue into a self-hosted runner; forbid it.
check "the plist declares no socket" '! grep -q "<key>Sockets</key>" "$PLIST"'
check "the plist is not a root daemon" '! grep -qi "UserName.*root" "$PLIST"'
check "the agent and worker clear inherited variables and use the sealed publisher" 'grep -A1 -F "<string>/usr/bin/env</string>" "$PLIST" | grep -Fq "<string>-i</string>" && grep -Fq '\''/usr/bin/env -i HOME="$HOME"'\'' "$WORKER" && grep -Fq '\''source "$REPO/scripts/live-gates/t17-worker-cleanup-fence.sh"'\'' "$WORKER" && grep -Fq '\''bridgevm_worker_publish_receipt "$tier"'\'' "$WORKER" && grep -Fq '\''"$worktree/scripts/live-gates/publish-receipt.sh" "$tier"'\'' "$FENCE" && grep -Fq '\''redact-receipt.py'\'' "$PUBLISH"'
no_match "no component opens a listening socket" \
    'nc +-l|socat|LISTEN|bind\(' "$CLI" "$WORKER" "$FENCE" "$TIER" "$T17_TIER" "$PUBLISH"
# Match registration rather than the installer guard's actions-runner name.
no_match "nothing registers a GitHub runner" \
    'config\.sh +--url|RUNNER_TOKEN|--runnergroup' "$CLI" "$WORKER" "$FENCE" "$INSTALL" "$TIER" "$T17_TIER" "$PUBLISH"
check "the installer refuses to sit beside a runner" \
    'grep -q "actions-runner" "$INSTALL"'
no_match "nothing in the queue path uses sudo" \
    '^[^#]*\bsudo\b' "$CLI" "$WORKER" "$FENCE" "$INSTALL" "$TIER" "$T17_TIER" "$PUBLISH"
check "live receipt policies pass" \
    '"$REPO/tests/integration/audio-teardown-live-tier-smoke.sh" | grep "PASS" >/dev/null'
check "the installed boot runner explicitly forwards the diagnostic request" 'grep -q '\''BRIDGEVM_HOST_DIAGNOSTIC_STOP_REQUEST='\'' "$BOOT_RUNNER"'
check "disk-harvest suppression fails closed for fresh title logs" \
    'grep -q "cannot be combined with title gates" "$REPO/scripts/run-hvf-windows-installed-boot-args.sh"'
    start=$(date +%s)
job_id="$("$CLI" submit t1-snapshot)"
elapsed=$(( $(date +%s) - start ))
check "submit returns a job id" '[ -n "$job_id" ]'
check "submit returns in under 10s" '[ "$elapsed" -lt 10 ]'
check "the job is queued" '"$CLI" status | grep -q "queued .*$job_id"'
check "the job records a commit" 'grep -q "^commit=[0-9a-f]\{40\}$" "$BRIDGEVM_LIVE_ROOT/queued/$job_id/job.env"'
check "the job records its tier" 'grep -q "^tier=t1-snapshot$" "$BRIDGEVM_LIVE_ROOT/queued/$job_id/job.env"'
bad_sha_id=Policy.bad_sha; check "submit rejects a malformed or unknown commit before burning an id" '! "$CLI" submit t1-snapshot --sha short --job-id "$bad_sha_id" >/dev/null 2>&1 && ! "$CLI" submit t1-snapshot --sha 0000000000000000000000000000000000000000 --job-id "$bad_sha_id" >/dev/null 2>&1 && [ ! -e "$BRIDGEVM_LIVE_ROOT/job-ledger/$bad_sha_id" ]'
ledger="$BRIDGEVM_LIVE_ROOT/job-ledger/$job_id/entry.env"; check "default submission burns a complete durable ledger entry" '[ "$(wc -l < "$ledger" | tr -d " ")" -eq 5 ] && grep -Fxq "job_id=$job_id" "$ledger" && grep -Fxq "tier=t1-snapshot" "$ledger" && grep -Eq "^commit=[0-9a-f]{40}$" "$ledger" && grep -Fxq "input_manifest_sha256=" "$ledger" && grep -Fxq "sealed_binary_sha256=" "$ledger"'
explicit_id=Policy.explicit_1-safe; explicit_result="$("$CLI" submit t1-snapshot --job-id "$explicit_id")"; check "a canonical explicit job id is preserved exactly" '[ "$explicit_result" = "$explicit_id" ] && [ -f "$BRIDGEVM_LIVE_ROOT/queued/$explicit_id/job.env" ]'
too_long="$(printf 'a%.0s' {1..129})"; check "noncanonical explicit job ids are rejected before publication" '! "$CLI" submit t1-snapshot --job-id ../escape >/dev/null 2>&1 && ! "$CLI" submit t1-snapshot --job-id -leading >/dev/null 2>&1 && ! "$CLI" submit t1-snapshot --job-id "$too_long" >/dev/null 2>&1'
rm -rf "$BRIDGEVM_LIVE_ROOT/queued/$explicit_id"; check "a burned id cannot be reused after its queue job disappears" '! "$CLI" submit t1-snapshot --job-id "$explicit_id" >/dev/null 2>&1'
partial_id=Policy.partial_1; mkdir -m 700 "$BRIDGEVM_LIVE_ROOT/job-ledger/$partial_id"; printf 'job_id=%s\n' "$partial_id" > "$BRIDGEVM_LIVE_ROOT/job-ledger/$partial_id/entry.env"; check "a partial ledger entry fails closed and permanently prevents reuse" '! "$CLI" submit t1-snapshot --job-id "$partial_id" >/dev/null 2>&1'
race_id=Policy.atomic_1; ("$CLI" submit t1-snapshot --job-id "$race_id" > "$WORK/race-a.out" 2>/dev/null) & race_a=$!; ("$CLI" submit t1-snapshot --job-id "$race_id" > "$WORK/race-b.out" 2>/dev/null) & race_b=$!
wait "$race_a" || true; wait "$race_b" || true
check "the ledger burn is atomic across concurrent submissions" '[ "$(grep -l "^$race_id$" "$WORK"/race-*.out 2>/dev/null | wc -l | tr -d " ")" -eq 1 ] && [ -d "$BRIDGEVM_LIVE_ROOT/queued/$race_id" ] && [ "$(wc -l < "$BRIDGEVM_LIVE_ROOT/job-ledger/$race_id/entry.env" | tr -d " ")" -eq 5 ]'; rm -rf "$BRIDGEVM_LIVE_ROOT/queued/$race_id"
# A sealed-input tier must seal a copied input manifest at submit time. It may
# not retain a caller-owned path that can be edited after submission.
manifest="$WORK/sealed-inputs.tsv"
printf 'image\t/tmp/image\t%s\n' "$(printf image | shasum -a 256 | cut -d' ' -f1)" > "$manifest"
printf '#!/bin/sh\nexit 0\n' > "$WORK/probe"; chmod +x "$WORK/probe"
printf 'binary\t%s\t%s\n' "$WORK/probe" "$(shasum -a 256 "$WORK/probe" | cut -d' ' -f1)" >> "$manifest"
sealed_job="$("$CLI" submit t18-audio-teardown --input-manifest "$manifest")"
check "sealed submit copies the input manifest" \
    '[ -f "$BRIDGEVM_LIVE_ROOT/queued/$sealed_job/input-manifest.tsv" ]'
check "sealed submit seals the copied manifest hash" \
    'grep -q "^input_manifest_sha256=[0-9a-f]\{64\}$" "$BRIDGEVM_LIVE_ROOT/queued/$sealed_job/job.env"'
check "sealed submit copies and seals the release binary and ledger hashes" \
    'test -x "$BRIDGEVM_LIVE_ROOT/queued/$sealed_job/hvf_gic_boot_probe" && grep -q "^sealed_binary_sha256=[0-9a-f]\{64\}$" "$BRIDGEVM_LIVE_ROOT/queued/$sealed_job/job.env" && cmp -s <(grep -E "^(input_manifest_sha256|sealed_binary_sha256)=" "$BRIDGEVM_LIVE_ROOT/queued/$sealed_job/job.env") <(grep -E "^(input_manifest_sha256|sealed_binary_sha256)=" "$BRIDGEVM_LIVE_ROOT/job-ledger/$sealed_job/entry.env")'
check "sealed submit rejects a missing manifest" \
    '! "$CLI" submit t18-audio-teardown --input-manifest "$WORK/missing" >/dev/null 2>&1'
check "unsealed tiers reject an input manifest" \
    '! "$CLI" submit t1-snapshot --input-manifest "$manifest" >/dev/null 2>&1'
check "the installed-boot runner honors a sealed prebuilt binary" \
    '[[ "$(head -n 1 "$BOOT_RUNNER")" == "source \"\$ROOT/scripts/run-hvf-windows-installed-boot-package-policy.sh\"" ]] && grep -Fq "BRIDGEVM_PREBUILT_PROBE requires an absolute regular release binary with --skip-build" "$REPO/scripts/run-hvf-windows-installed-boot-package-policy.sh"'
rm -rf "$BRIDGEVM_LIVE_ROOT/queued/$sealed_job"
claimed="$("$CLI" next)"
check "next claims the job" '[ -n "$claimed" ]'
check "the claimed job moved to running" '"$CLI" status | grep -q "running .*$job_id"'
check "a second worker cannot claim it" '! "$CLI" next'

# A sole worker may be killed by logout or launchd. Its next generation must
# retain the abandoned job as a failure instead of leaving it running forever.
recovery_queue="$WORK/recovery-queue"; stale="$recovery_queue/running/stale-job"
mkdir -p "$stale" "$recovery_queue/done" "$WORK/recovery-work"
printf 'job_id=stale-job\ntier=t1-snapshot\ncommit=%040d\n' 0 > "$stale/job.env"
"$RECOVER" "$REPO" "$recovery_queue" "$WORK/recovery-work" 2>/dev/null
check "an abandoned running job is retained as interrupted" \
    'grep -q "^result=interrupted-worker-exit$" "$recovery_queue/done/stale-job/result.env"'
check "the stale job no longer claims to be running" '[ ! -e "$stale" ]'

source "$REPO/tests/integration/live_gate_receipt_redaction_cases.sh" "$claimed"

check "cancelling a running job requests, not kills" \
    '"$CLI" cancel "$job_id" | grep -q "cancellation requested"'
check "the cancel request is visible to the worker" '[ -f "$claimed/cancel.requested" ]'

second="$("$CLI" submit t1-snapshot)"
check "a queued job cancels immediately" '"$CLI" cancel "$second" | grep -q "canceled"'
check "the canceled job is done" '"$CLI" status | grep -q "done .*$second"'

check "an unknown tier is rejected" '! "$TIER" nonsense --out "$WORK/x" 2>/dev/null'
check "the installer supports a dry run" 'python3 "$REPO/tests/integration/live-installer-dry-run-contract.py" && python3 "$REPO/tests/integration/live-installer-launch-contract.py" && python3 "$REPO/tests/integration/live-installer-storage-contract.py" && python3 "$REPO/tests/integration/live-storage-capacity-contract.py" && python3 "$REPO/tests/integration/live-storage-queue-alias-contract.py" && python3 "$REPO/tests/integration/live-worker-lock-quoting-contract.py"'
check "the installer guards LaunchAgent privacy-protected source paths" 'grep -q "LaunchAgent privacy policy" "$INSTALL"'
no_match "the installer stores no credentials" \
    'password|token=|api[_-]key' "$INSTALL"
check "deterministic checks stay off the live queue" '"$REPO/tests/integration/live-deterministic-venue-smoke.sh" | grep -q "PASS"'
echo "PASS: live gate policy smoke ($checks checks)"
