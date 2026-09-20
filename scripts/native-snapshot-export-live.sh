native_snapshot_export_and_select() { # cli, vm id, library, work, retained output
  local cli=$1 vm_id=$2 library=$3 work=$4 output=$5
  local export=$work/export.snapshot
  "$cli" app snapshot-export "$vm_id" "$export" --library "$library" --json \
    > "$output/export.json" 2> "$output/export.stderr" \
    || { echo "native snapshot export failed; see $output/export.stderr" >&2; return 1; }
  python3 "$REPO/scripts/live-gates/native_snapshot_export_evidence.py" \
    "$output/export.json" "$export" "$vm_id" "$library" \
    > "$output/export-evidence.json" \
    || { echo "native snapshot export evidence is invalid" >&2; return 1; }
  WORK_DISK=$export/disk.raw
  WORK_VARS=$export/vars.fd
}
