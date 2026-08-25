# A zero exit with no receipt measured nothing. Sourced by
# live-gate-policy-smoke.sh; uses its `check` helper, $REPO and $WORK.
finalize_case() { # dir tier exit-code -> prints "result receipt_pass"
    ( set -uo pipefail; log() { :; }
      source "$REPO/scripts/live-gates/worker-job-finalize.sh"
      dir="$1"; tier="$2"; job_id=J; commit=C; worktree="$REPO"; status=0
      printf 'x\n' > "$dir/job.env"; ( exit "$3" ) & tier_pid=$!
      supervise_and_finalize
      printf '%s %s\n' "$(awk -F= '$1=="result"{print $2}' "$dir/result.env")" \
        "$(python3 -c "import json;print(json.load(open('$dir/receipt.json'))['pass'])")" )
}
fin_a="$(mkdir -p "$WORK/fa" && finalize_case "$WORK/fa" t10-qmp-stress 0)"
mkdir -p "$WORK/fb"; printf '{"tier":"t10-qmp-stress","pass":true,"sample_count":60}' > "$WORK/fb/receipt.json"
fin_b="$(finalize_case "$WORK/fb" t10-qmp-stress 0)"
fin_c="$(mkdir -p "$WORK/fc" && finalize_case "$WORK/fc" t8-pointer-reliability 3)"
check "a zero exit without a receipt is recorded as a failure" '[ "$fin_a" = "fail False" ]'
check "a real passing receipt still passes" '[ "$fin_b" = "pass True" ]'
check "a nonzero exit fails with a fail-closed receipt" '[ "$fin_c" = "fail False" ]'
check "every live tier has a fail-closed receipt shape" 'for t in t8-pointer-reliability t9-audio-teardown t10-qmp-stress t11-glyph-scene-pilot t12-b4-umd-diagnostic; do d="$WORK/shape-$t"; mkdir -p "$d"; printf "x\n" > "$d/job.env"; "$REPO/scripts/live-gates/write-missing-receipt.sh" "$t" "$d" "$REPO" J C || exit 1; python3 -c "import json,sys;r=json.load(open(sys.argv[1]));sys.exit(0 if r[\"pass\"] is False and r[\"tier\"]==sys.argv[2] else 1)" "$d/receipt.json" "$t" || exit 1; done'
recover_root="$WORK/recover"; recover_work="$WORK/recover-work"
mkdir -p "$recover_root/running/orphan" "$recover_root/done" "$recover_work"
printf 'job_id=orphan\ntier=t10-qmp-stress\ncommit=deadbeef\n' >"$recover_root/running/orphan/job.env"
BRIDGEVM_LIVE_ROOT="$recover_root" BRIDGEVM_LIVE_WORK="$recover_work" BRIDGEVM_REPO="$REPO" "$REPO/scripts/live-gates/recover-live-jobs.sh"
check "an abandoned running job is recovered as failed evidence" '[ -f "$recover_root/done/orphan/receipt.public.json" ] && grep -q "^result=fail$" "$recover_root/done/orphan/result.env" && grep -q '"'"'"pass": false'"'"' "$recover_root/done/orphan/receipt.public.json"'
