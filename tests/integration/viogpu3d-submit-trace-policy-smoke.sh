#!/usr/bin/env bash
set -euo pipefail; ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
PATCH="$ROOT/scripts/patches/virtio-win-mesa-submit-trace.patch"
grep -Fq 'BV-VIRGL-ALLOC-LIST-GROW-FAIL alloc_count=%d max_alloc=%u res_id=%u' "$PATCH"
[[ $(grep -Fc 'InterlockedIncrement(&emitted) > ' "$PATCH") == 2 ]]; grep -Fq 'InterlockedIncrement(&emitted) > 32' "$PATCH"; grep -Fq 'InterlockedIncrement(&emitted) > 64' "$PATCH"; grep -Fq 'InterlockedExchange(&bridgevm_trace_grow_failed, 1)' "$PATCH"; grep -Fq 'InterlockedCompareExchange(&bridgevm_trace_grow_failed, 0, 0)' "$PATCH"
grep -Fq 'allocations=%u max_alloc=%u d3d_list_size=%u' "$PATCH"
! grep -E 'BV-VIRGL-(ALLOC-LIST-GROW-FAIL|SUBMIT).*\b(address|gpa|payload|command_bytes)=' "$PATCH"; grep -Fqx 'scripts/patches/virtio-win-mesa-submit-trace.patch text eol=lf whitespace=-blank-at-eol' "$ROOT/.gitattributes"
SOURCE="$HOME/BridgeVM/src/virtio-win-mesa-cb531c44"; if [[ -d "$SOURCE/.git" ]]; then
  work="$(mktemp -d)"; trap 'rm -rf "$work"' EXIT
  git -C "$SOURCE" archive HEAD | tar -x -C "$work"; (cd "$work" && git apply "$ROOT/scripts/patches/virtio-win-mesa-unbound-clear.patch" && git apply --check "$PATCH" && git apply "$PATCH" && grep -B2 -A2 -F 'NTSTATUS Status = cbuf->ctx->render(cbuf->ctx, &render);' src/gallium/winsys/virgl/gdi/virgl_gdi_winsys.c | grep -q 'bridgevm_trace_submit("before-render"')
fi
echo 'PASS: bounded payload-free UMD allocation-list trace policy'
