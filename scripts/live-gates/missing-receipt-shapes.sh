# Fail-closed receipt shapes for tiers whose receipt is one flat JSON object.
# Every tier must appear here or in write-missing-receipt.sh: a tier with no
# case leaves a died-early job with no receipt, and the worker then has no
# evidence to contradict a zero exit status.
flat_receipt_fields() { # tier -> "gate_id criterion" + $extra
  case "$1" in
    t8-pointer-reliability)
      gate=b4-pointer-click-reliability; criterion=B4
      extra='"image_sha256":"absent","vars_sha256":"absent","sample_count":0,"landed":"0/20","p95_first_changed_ms":"unknown",' ;;
    t9-audio-teardown)
      gate=a5-audio-teardown-quality; criterion=A5-quality
      extra='"image_sha256":"absent","vars_sha256":"absent","sample_count":0,"required_run_count":10,"passes":0,' ;;
    t10-qmp-stress)
      gate=qmp-close-race-stress; criterion=QMP
      extra='"sample_count":0,"required_run_count":60,"passes":0,"failures":0,' ;;
    t11-glyph-scene-pilot)
      gate=glyph-scene-channel; criterion=glyph-diagnostic-only; extra='"sample_count":0,' ;;
    t12-b4-umd-diagnostic)
      gate=b4-umd-host-resource-correlation; criterion=B4; extra='"sample_count":0,' ;;
    t13-compatibility-observation) gate=windows-20-workload-observation; criterion=compatibility-diagnostic-only; extra='"sample_count":0,"required_run_count":20,"identities_verified":0,' ;;
    *) gate=queue-tier-failure; criterion="$1"; extra='"sample_count":0,' ;;
  esac
}
python_receipt() { # tier dir worktree job commit reason -> 0 when handled
  case "$1" in
    t6-a3-title)
      python3 "$3/scripts/live-gates/write-a3-title-receipt.py" \
        --out "$2" --job-id "$4" --commit "$5" --reason "$6" || true
      [[ -f "$2/receipt.json" ]] ;;
    t7-windows-closure)
      python3 "$3/scripts/live-gates/write-windows-closure-receipt.py" \
        --out "$2" --job-id "$4" --commit "$5" \
        --input-manifest-hash "$(awk -F= '$1=="input_manifest_sha256"{print $2}' "$2/job.env")" \
        --reason "$6" || true
      [[ -f "$2/receipt.json" ]] ;;
    *) return 1 ;;
  esac
}
