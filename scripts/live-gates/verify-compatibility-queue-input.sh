#!/usr/bin/env bash
set -euo pipefail
DIR="$1"; REPO="$2"; MANIFEST="$DIR/input-manifest.tsv"
expected=$(awk -F= '$1=="compatibility_candidates_sha256"{print $2}' "$DIR/job.env")
actual=$(python3 "$REPO/scripts/live-gates/compatibility-observation-input.py" verify \
  --manifest "$MANIFEST" --dir "$DIR/sealed-compatibility" --verify-large 2>/dev/null || true)
[[ -n "$expected" && "$actual" == "$expected" ]] || exit 1
expected=$(awk -F= '$1=="sealed_package_sha256"{print $2}' "$DIR/job.env")
actual=$(python3 "$REPO/scripts/live-gates/b4-diagnostic-package.py" verify \
  --manifest "$DIR/sealed-compatibility/b4-input-manifest.tsv" --dir "$DIR/sealed-package" 2>/dev/null || true)
[[ -n "$expected" && "$actual" == "$expected" ]]
