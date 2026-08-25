#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"; cd "$ROOT"
PREFLIGHT=scripts/win-assets/bvgpu-driver-preflight.ps1
FIRSTBOOT=scripts/win-assets/bvgpu-firstboot.cmd
BUILDER=scripts/build-hvf-windows-driver-injector.sh; INJECTOR=scripts/win-assets/bvinject.cmd
fail() { echo "FAIL: $*" >&2; exit 1; }
pwsh -NoProfile -File "$PREFLIGHT" -SelfTest \
  | grep -q '^PASS: bvgpu preflight self-test (8 cases)$' || fail 'preflight self-test'
for mutation in 'bcdedit /set' 'certutil ' 'pnputil ' 'Remove-Item' 'dism '; do
  grep -Fqi "$mutation" "$PREFLIGHT" && fail "preflight mutates with $mutation"
done
stage1="$(awk '{sub(/\r$/,"")} $0==":stage1"{p=1} p{print} $0==":stage2"{exit}' "$FIRSTBOOT")"
before() {
  local first second
  first="$(grep -nF "$1" <<<"$stage1" | head -1 | cut -d: -f1)"
  second="$(grep -nF "$2" <<<"$stage1" | head -1 | cut -d: -f1)"
  [[ -n "$first" && -n "$second" && "$first" -lt "$second" ]] || fail "$1 must precede $2"
}
before bvgpu-driver-preflight.ps1 bvgpu-clean-driver-state.ps1; before bvgpu-driver-preflight.ps1 'bcdedit /set'; before bvgpu-driver-preflight.ps1 'certutil -f -addstore'
grep -Fq 'bvgpu-clean-driver-state.ps1 bvgpu-driver-preflight.ps1' "$BUILDER" \
  && grep -Fq 'cp "$ASSETS/$asset" "$DST_VOL/$asset"' "$BUILDER" \
  || fail 'builder does not stage preflight'
grep -Fq 'copy /y %DRV%\..\bvgpu-driver-preflight.ps1' "$INJECTOR" \
  || fail 'injector does not copy preflight'
grep -Fq 'driver_preflight_blocker=' scripts/run-hvf-windows-installed-boot-runner.sh && grep -Fq 'kernel-policy-provenance-unverifiable' "$PREFLIGHT" \
  || fail 'exact fail-closed blockers are not surfaced'
echo 'PASS: driver signing/Secure Boot preflight is read-only and first'
