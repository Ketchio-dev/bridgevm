#!/usr/bin/env bash
# Source only: materialize and hash the same selected pair the native VM reads.
snapshot_selected_hashes() {
  "$CLI" create "$WORK/disk.raw" "$WORK/vars.fd" "$WORK/selected-check" a19-selected-check "$QUOTA" \
    >> "$RECEIPT" 2>&1 || fail "could not capture the current selected pair"
  after_disk=$(sha256 "$WORK/selected-check/disk.raw")
  after_vars=$(sha256 "$WORK/selected-check/vars.fd")
  [[ "$after_disk" =~ ^[0-9a-f]{64}$ && "$after_vars" =~ ^[0-9a-f]{64}$ ]] \
    || fail "selected pair hashing failed"
}
