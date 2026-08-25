#!/usr/bin/env bash
set -euo pipefail; ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
PATCH="$ROOT/scripts/patches/virtio-win-mesa-submit-trace.patch"
grep -Fq 'BV-VIRGL-ALLOC-LIST-GROW-FAIL alloc_count=%d max_alloc=%u res_id=%u' "$PATCH"
grep -Fq 'InterlockedIncrement(&emitted) > 32' "$PATCH"
grep -Fq 'allocations=%u max_alloc=%u d3d_list_size=%u' "$PATCH"
! grep -E 'BV-VIRGL-(ALLOC-LIST-GROW-FAIL|SUBMIT).*\b(address|gpa|payload|command_bytes)=' "$PATCH"
SOURCE="$HOME/BridgeVM/src/virtio-win-mesa-cb531c44"; if [[ -d "$SOURCE/.git" ]]; then
  work="$(mktemp -d)"; trap 'rm -rf "$work"' EXIT
  git -C "$SOURCE" archive HEAD | tar -x -C "$work"; (cd "$work" && git apply "$ROOT/scripts/patches/virtio-win-mesa-unbound-clear.patch" && git apply --check "$PATCH")
fi
echo 'PASS: bounded payload-free UMD allocation-list trace policy'
