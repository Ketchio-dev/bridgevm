receipt_claimed=${1:?claimed T1 job directory required}
receipt_queue="$WORK/redaction-queue"
receipt_job=policy-a3-redaction
receipt_dir="$receipt_queue/running/$receipt_job"
receipt_commit=0123456789abcdef0123456789abcdef01234567
mkdir -p "$receipt_dir" "$receipt_queue/job-ledger/$receipt_job"
printf 'job_id=%s\ntier=t6-a3-title\ncommit=%s\n' "$receipt_job" "$receipt_commit" > "$receipt_dir/job.env"
cp "$receipt_dir/job.env" "$receipt_queue/job-ledger/$receipt_job/entry.env"
chmod 400 "$receipt_queue/job-ledger/$receipt_job/entry.env"
a3_receipt() {
    BRIDGEVM_LIVE_ROOT="$receipt_queue" "$CLI" receipt "$receipt_job"
}
cat > "$receipt_dir/receipt.json" <<'JSON'
{"gate_id":"a3-d3d11-real-title-3run","criterion":"A3","tested_commit":"0123456789abcdef0123456789abcdef01234567","tier":"t6-a3-title","pass":true,"passes":3,"sample_count":1200,"fps_p50":[58.82,58.82,58.82],"title_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","evidence_paths":["run-1/summary.txt"],"disk_path":"/Users/me/win11.qcow2","vars_path":"/tmp/VARS.fd"}
JSON
check "a raw A3 receipt is never served" '! a3_receipt >/dev/null 2>&1'
python3 "$REDACT" --in "$receipt_dir/receipt.json" --out "$receipt_dir/receipt.public.json"
check "the published A3 receipt is served under its own identity" 'a3_receipt >/dev/null'
check "the published receipt drops the disk path" '! a3_receipt | grep -q "qcow2"'
check "the published receipt drops the vars path" '! a3_receipt | grep -q "VARS.fd"'
check "the published receipt keeps the result" 'a3_receipt | grep -q "\"pass\": true"'
check "the published receipt keeps A3 provenance" 'a3_receipt | grep -q "\"criterion\": \"A3\""'
check "the published receipt keeps FPS samples" 'a3_receipt | grep -q "\"sample_count\": 1200"'
check "the published receipt keeps only relative evidence paths" 'a3_receipt | grep -q "run-1/summary.txt"'
cp "$receipt_dir/receipt.json" "$receipt_claimed/receipt.json"
check "a raw receipt is never served for the claimed T1 job" '! "$CLI" receipt "$job_id" >/dev/null 2>&1'
cp "$receipt_dir/receipt.public.json" "$receipt_claimed/receipt.public.json"
check "a valid A3 payload cannot be served under T1 identity" '! "$CLI" receipt "$job_id" > "$WORK/cross-tier.out" 2>/dev/null && [ ! -s "$WORK/cross-tier.out" ]'
