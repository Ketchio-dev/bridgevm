#!/usr/bin/env bash
# Bind one exact performance binary digest to its hosted GitHub source build.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd -P)"
REPOSITORY=Ketchio-dev/bridgevm
SIGNER_WORKFLOW=Ketchio-dev/bridgevm/.github/workflows/hvf-performance-binary.yml
BINARY=""; SOURCE_SHA=""; EXPECTED_SHA256=""

usage() {
  echo "usage: $0 --binary PATH --source-sha 40_HEX --sha256 64_HEX" >&2
}
fail() { echo "HVF performance binary provenance: $*" >&2; exit 1; }

self_test() {
  local temporary binary digest fake args workflow rewrite_line sign_line attest_line self
  temporary="$(mktemp -d "${TMPDIR:-/tmp}/bridgevm-performance-provenance.XXXXXX")"
  trap 'rm -rf "$temporary"' RETURN
  binary="$temporary/hvf_gic_boot_probe"; printf 'attested fixture\n' > "$binary"
  digest="$(shasum -a 256 "$binary" | cut -d' ' -f1)"
  fake="$temporary/gh"; args="$temporary/args"
  printf '%s\n' '#!/usr/bin/env bash' 'printf "%s\n" "$@" > "$BRIDGEVM_TEST_GH_ARGS"' \
    'exit "${BRIDGEVM_TEST_GH_EXIT:-0}"' > "$fake"
  chmod 700 "$fake"
  self="$(cd "$(dirname "$0")" && pwd -P)/$(basename "$0")"
  PATH="$temporary:$PATH" BRIDGEVM_TEST_GH_ARGS="$args" "$self" --binary "$binary" \
    --source-sha "$(printf 'a%.0s' {1..40})" --sha256 "$digest" >/dev/null
  grep -Fxq attestation "$args"; grep -Fxq verify "$args"
  grep -Fxq -- --deny-self-hosted-runners "$args"
  grep -Fxq -- --source-digest "$args"; grep -Fxq "$(printf 'a%.0s' {1..40})" "$args"
  grep -Fxq -- --signer-workflow "$args"; grep -Fxq "$SIGNER_WORKFLOW" "$args"
  grep -Fxq -- --repo "$args"; grep -Fxq "$REPOSITORY" "$args"
  if PATH="$temporary:$PATH" BRIDGEVM_TEST_GH_ARGS="$args" "$self" --binary "$binary" \
    --source-sha "$(printf 'a%.0s' {1..40})" --sha256 "$(printf '0%.0s' {1..64})" >/dev/null 2>&1; then return 1; fi
  ln -s "$binary" "$temporary/symlink"
  if PATH="$temporary:$PATH" "$self" --binary "$temporary/symlink" \
    --source-sha "$(printf 'a%.0s' {1..40})" --sha256 "$digest" >/dev/null 2>&1; then return 1; fi
  if PATH="$temporary:$PATH" BRIDGEVM_TEST_GH_ARGS="$args" BRIDGEVM_TEST_GH_EXIT=1 "$self" \
    --binary "$binary" --source-sha "$(printf 'a%.0s' {1..40})" --sha256 "$digest" >/dev/null 2>&1; then return 1; fi
  workflow="$ROOT/.github/workflows/hvf-performance-binary.yml"
  grep -Fq 'workflow_dispatch:' "$workflow"; ! grep -Eq '^  (push|pull_request):' "$workflow"
  grep -Fq 'runs-on: macos-15' "$workflow"; ! grep -Fq 'self-hosted' "$workflow"
  grep -Fq 'id-token: write' "$workflow"; grep -Fq 'attestations: write' "$workflow"
  grep -Fq 'actions/attest@508db95dd578ae2727ebd6217d5ba78e4fbda05d' "$workflow"
  grep -Fq '/Users/user/BridgeVM/3d/prefix/lib/libvirglrenderer.1.dylib' "$workflow"
  rewrite_line="$(grep -n 'install_name_tool -change' "$workflow" | cut -d: -f1)"
  sign_line="$(grep -n 'codesign --force --sign -' "$workflow" | cut -d: -f1)"
  attest_line="$(grep -n 'uses: actions/attest@' "$workflow" | cut -d: -f1)"
  (( rewrite_line < sign_line && sign_line < attest_line ))
  echo "HVF performance binary provenance self-test: PASS"
}

[[ "${1:-}" != --self-test ]] || { [[ $# == 1 ]] || { usage; exit 2; }; self_test; exit; }
while [[ $# -gt 0 ]]; do
  case "$1" in
    --binary) [[ $# -ge 2 ]] || { usage; exit 2; }; BINARY="$2"; shift 2 ;;
    --source-sha) [[ $# -ge 2 ]] || { usage; exit 2; }; SOURCE_SHA="$2"; shift 2 ;;
    --sha256) [[ $# -ge 2 ]] || { usage; exit 2; }; EXPECTED_SHA256="$2"; shift 2 ;;
    *) usage; exit 2 ;;
  esac
done
[[ -f "$BINARY" && ! -L "$BINARY" ]] || fail "binary must be a regular, non-symlink file"
[[ "$SOURCE_SHA" =~ ^[0-9a-f]{40}$ ]] || fail "source SHA must be 40 lowercase hex characters"
[[ "$EXPECTED_SHA256" =~ ^[0-9a-f]{64}$ ]] || fail "SHA-256 must be 64 lowercase hex characters"
actual="$(shasum -a 256 "$BINARY" | cut -d' ' -f1)"
[[ "$actual" == "$EXPECTED_SHA256" ]] || fail "binary bytes do not match the expected SHA-256"
gh_bin="$(command -v gh || true)"; [[ -n "$gh_bin" ]] || fail "GitHub CLI is required"
"$gh_bin" attestation verify "$BINARY" --repo "$REPOSITORY" \
  --signer-workflow "$SIGNER_WORKFLOW" --source-digest "$SOURCE_SHA" \
  --deny-self-hosted-runners >/dev/null || fail "GitHub attestation verification failed"
echo "HVF performance binary provenance: PASS"
