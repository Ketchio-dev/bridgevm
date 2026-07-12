#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

STORE="$(mktemp -d "/tmp/bridgevm-viogpu3d-render-stage.XXXXXX")"
SOURCE_INX="$STORE/viogpu3d_arm64.inx"
INPUT="$STORE/input"
MANIFEST="$STORE/input.sha256"
OUT="$STORE/out"
CHECKER_FIXTURE="$STORE/checker-fixture"

mkdir -p "$INPUT"

fail() {
  echo "FAIL: $*" >&2
  echo "Store preserved at $STORE" >&2
  exit 1
}

assert_contains() {
  local haystack="$1"
  local needle="$2"
  local label="$3"
  case "$haystack" in
    *"$needle"*) ;;
    *) fail "$label missing '$needle'; got: $haystack" ;;
  esac
}

assert_file_contains() {
  local path="$1"
  local needle="$2"
  local label="$3"
  [[ -f "$path" ]] || fail "$label file missing: $path"
  grep -Fq -- "$needle" "$path" || fail "$label missing '$needle' in $path"
}

assert_fails_contains() {
  local needle="$1"
  shift
  local output
  if output="$("$@" 2>&1)"; then
    fail "command unexpectedly succeeded: $*"
  fi
  assert_contains "$output" "$needle" "expected failure"
}

write_minimal_arm64_pe() {
  local path="$1"
  dd if=/dev/zero of="$path" bs=512 count=1 >/dev/null 2>&1
  printf 'MZ' | dd of="$path" bs=1 seek=0 conv=notrunc >/dev/null 2>&1
  printf '\200\000\000\000' | dd of="$path" bs=1 seek=60 conv=notrunc >/dev/null 2>&1
  printf 'PE\000\000\144\252' | dd of="$path" bs=1 seek=128 conv=notrunc >/dev/null 2>&1
}

cat > "$SOURCE_INX" <<'EOF'
[DestinationDirs]
VioGpu3D_Files.Usermode = 11
[SourceDisksFiles]
viogpu_d3d10_arm64.dll = 1,,
viogpu_wgl_arm64.dll = 1,,
opengl32_arm64.dll = 1,,
libEGL_arm64.dll = 1,,
libGLESv2_arm64.dll = 1,,
[Models]
Device = Install, PCI\VEN_1AF4&DEV_1050
[Settings]
HKR,,UserModeDriverName,0x00010000,%11%\viogpu_d3d10.dll,%11%\viogpu_d3d10.dll,%11%\viogpu_d3d10.dll,%11%\viogpu_d3d10.dll
HKR,,OpenGLDriverName,0x00010000,%11%\viogpu_wgl.dll
HKR,,OpenGLFlags,0x00010001,3
HKR,,OpenGLVersion,0x00010001,4096
HKR,,InstalledDisplayDrivers,0x00010000,viogpu_d3d10,viogpu_d3d10,viogpu_d3d10
EOF

printf '; original CI fallback INF\n' > "$INPUT/viogpu3d.inf"
for name in \
  viogpu3d.sys \
  viogpu_d3d10_arm64.dll \
  viogpu_wgl_arm64.dll \
  opengl32_arm64.dll \
  libEGL_arm64.dll \
  libGLESv2_arm64.dll
do
  write_minimal_arm64_pe "$INPUT/$name"
done
printf 'stale catalog must not be copied\n' > "$INPUT/viogpu3d.cat"
printf 'stale certificate must not be copied\n' > "$INPUT/stale.cer"

(
  cd "$INPUT"
  for name in \
    libEGL_arm64.dll \
    libGLESv2_arm64.dll \
    opengl32_arm64.dll \
    viogpu3d.inf \
    viogpu3d.sys \
    viogpu_d3d10_arm64.dll \
    viogpu_wgl_arm64.dll
  do
    shasum -a 256 "$name"
  done
) > "$MANIFEST"

source_hash="$(shasum -a 256 "$SOURCE_INX" | awk '{print $1}')"
output="$(
  bash scripts/stage-hvf-windows-viogpu3d-render-package.sh \
    --input-dir "$INPUT" \
    --source-inx "$SOURCE_INX" \
    --source-head fixture-head \
    --expected-source-head fixture-head \
    --expected-source-inx-sha256 "$source_hash" \
    --expected-input-manifest "$MANIFEST" \
    --source-repo fixture/viogpu3d \
    --out-dir "$OUT" 2>&1
)" || fail "render stage failed: $output"

assert_contains "$output" "profile=arehnman-arm64-minimal" "stage output"
assert_contains "$output" "finalization_required=true" "stage output"
assert_contains "$output" "render_candidate_verified=false" "stage output"
[[ -d "$OUT/package" ]] || fail "staged package directory missing"
[[ ! -e "$OUT/package/viogpu3d.cat" ]] || fail "stale catalog leaked into stage"
[[ ! -e "$OUT/package/stale.cer" ]] || fail "stale certificate leaked into stage"

assert_file_contains "$OUT/package/viogpu3d.inf" 'PCI\VEN_1AF4&DEV_1050' "staged INF"
assert_file_contains "$OUT/package/viogpu3d.inf" 'VioGpu3D_Files.Usermode=11' "staged INF"
assert_file_contains "$OUT/package/viogpu3d.inf" 'UserModeDriverName,%REG_MULTI_SZ%,%11%\viogpu_d3d10.dll,%11%\viogpu_d3d10.dll,%11%\viogpu_d3d10.dll,%11%\viogpu_d3d10.dll' "staged INF"
assert_file_contains "$OUT/package/viogpu3d.inf" 'OpenGLDriverName,%REG_MULTI_SZ%,%11%\viogpu_wgl.dll' "staged INF"
assert_file_contains "$OUT/package/viogpu3d.inf" 'InstalledDisplayDrivers,%REG_MULTI_SZ%,viogpu_d3d10,viogpu_d3d10,viogpu_d3d10' "staged INF"
assert_file_contains "$OUT/package/viogpu3d.inf" 'CopyFiles=VioGpu3D_Files.Driver,VioGpu3D_Files.Usermode' "staged INF"
assert_file_contains "$OUT/package/bridgevm-package-provenance.env" 'VIOGPU3D_SIGNING_CERT=<pending-windows-wdk-finalization>' "staged provenance"
assert_file_contains "$OUT/stage-report.txt" 'package_capability=unsigned-umd-registered-stage' "stage report"
assert_file_contains "$OUT/stage-report.txt" 'catalog_present=false' "stage report"
assert_file_contains "$OUT/stage-report.txt" 'pre_finalization_manifest=pre-finalization-sha256.txt' "stage report"
assert_file_contains "$OUT/stage-report.txt" 'default_finalized_dir=package-finalized' "stage report"
assert_file_contains "$OUT/stage-report.txt" 'canonical_inf_sha256=f8bc2e3bb097d1d8f9d461745dc6665b65bddf53cbb986dc57df1059f374b5e9' "stage report"
assert_file_contains "$OUT/README.txt" '-PreFinalizationManifest .\pre-finalization-sha256.txt' "stage readme"
assert_file_contains "$OUT/README.txt" 'unsigned package/ input is never modified' "stage readme"
assert_file_contains "$OUT/README.txt" '.\finalize-viogpu3d-test-package.ps1' "stage readme"
assert_file_contains "$OUT/README.txt" 'WDK supplies InfVerif and Inf2Cat' "stage readme"
assert_file_contains "$OUT/README.txt" 'Windows SDK supplies SignTool' "stage readme"
assert_file_contains "$OUT/README.txt" 'test_signing_required=true' "stage readme"
assert_file_contains "$OUT/README.txt" 'not a Microsoft kernel-policy root' "stage readme"

TEST_FINALIZER="$OUT/finalize-viogpu3d-test-package.ps1"
assert_file_contains "$TEST_FINALIZER" 'New-SelfSignedCertificate' "Windows test finalizer"
assert_file_contains "$TEST_FINALIZER" 'certutil.exe @Arguments' "Windows test finalizer native certificate-store path"
assert_file_contains "$TEST_FINALIZER" '@("-f", "-addstore", "Root", $temporaryCer)' "Windows test finalizer root import"
assert_file_contains "$TEST_FINALIZER" '@("-f", "-addstore", "TrustedPublisher", $temporaryCer)' "Windows test finalizer publisher import"
assert_file_contains "$TEST_FINALIZER" 'Remove-Item -LiteralPath $temporaryPfx' "Windows test finalizer"
assert_file_contains "$TEST_FINALIZER" '$finalizationSucceeded = $true' "Windows test finalizer"
assert_file_contains "$TEST_FINALIZER" '-TestSigning' "Windows test finalizer explicit signing mode"
assert_file_contains "$TEST_FINALIZER" 'test_signing_required=true' "Windows test finalizer report contract"
assert_file_contains "$TEST_FINALIZER" '@("-delstore", "Root", $certificate.Thumbprint)' "Windows test finalizer root cleanup"
assert_file_contains "$TEST_FINALIZER" '@("-delstore", "TrustedPublisher", $certificate.Thumbprint)' "Windows test finalizer publisher cleanup"

FINALIZER="$OUT/finalize-viogpu3d-package.ps1"
assert_file_contains "$FINALIZER" 'Assert-PinnedInputManifest' "Windows finalizer"
assert_file_contains "$FINALIZER" 'f8bc2e3bb097d1d8f9d461745dc6665b65bddf53cbb986dc57df1059f374b5e9' "Windows finalizer"
assert_file_contains "$FINALIZER" 'InfVerif.exe' "Windows finalizer"
assert_file_contains "$FINALIZER" 'Windows Kits\10\bin' "Windows finalizer"
assert_file_contains "$FINALIZER" 'Windows Kits\10\Tools' "Windows finalizer"
assert_file_contains "$FINALIZER" 'PROCESSOR_ARCHITEW6432' "Windows finalizer"
assert_file_contains "$FINALIZER" 'supplies InfVerif/Inf2Cat' "Windows finalizer"
assert_file_contains "$FINALIZER" 'Windows SDK supplies SignTool' "Windows finalizer"
assert_file_contains "$FINALIZER" '[string]$Inf2CatOs = "auto"' "Windows finalizer Inf2Cat OS selection"
assert_file_contains "$FINALIZER" 'Resolve-Inf2CatOperatingSystem' "Windows finalizer Inf2Cat OS selection"
assert_file_contains "$FINALIZER" '"10_GE_ARM64"' "Windows finalizer current ARM64 OS token"
assert_file_contains "$FINALIZER" '"10_RS3_ARM64"' "Windows finalizer oldest supported ARM64 fallback"
assert_file_contains "$FINALIZER" '"/sm", "/sha1", $CertificateThumbprint' "Windows finalizer machine-store signing"
assert_file_contains "$FINALIZER" '[Environment]::SetEnvironmentVariable(' "Windows finalizer child-environment secret suppression"
assert_file_contains "$FINALIZER" 'signing_source=$signingSource' "Windows finalizer signing-source report"
assert_file_contains "$FINALIZER" '[switch]$TestSigning' "Windows finalizer explicit test-signing mode"
assert_file_contains "$FINALIZER" 'test_signing_required=$testSigningRequired' "Windows finalizer signing-mode report"
assert_file_contains "$FINALIZER" 'skipped-self-signed-test-root' "Windows finalizer honest kernel-policy report"
assert_file_contains "$FINALIZER" 'Sign-Artifact -Path $Sys' "Windows finalizer"
assert_file_contains "$FINALIZER" 'Sign-Artifact -Path $dll.FullName' "Windows finalizer"
assert_file_contains "$FINALIZER" 'Inf2Cat.exe' "Windows finalizer"
assert_file_contains "$FINALIZER" 'Sign-Artifact -Path $Cat' "Windows finalizer"
assert_file_contains "$FINALIZER" '"verify", "/v", "/kp", $Sys' "Windows finalizer"
assert_file_contains "$FINALIZER" '"verify", "/v", "/kp", $Cat' "Windows finalizer"
assert_file_contains "$FINALIZER" '[System.IO.Directory]::Move($workingPackageDir, $FinalizedDir)' "Windows finalizer"
assert_file_contains "$FINALIZER" 'finalization_complete=true' "Windows finalizer"
assert_file_contains "$FINALIZER" 'dll_authenticode_verified=true' "Windows finalizer"

sys_sign_line="$(grep -nF 'Sign-Artifact -Path $Sys' "$FINALIZER" | head -n 1 | cut -d: -f1)"
dll_sign_line="$(grep -nF 'Sign-Artifact -Path $dll.FullName' "$FINALIZER" | head -n 1 | cut -d: -f1)"
inf2cat_line="$(grep -nF 'Invoke-ExternalTool -Tool $Inf2Cat' "$FINALIZER" | head -n 1 | cut -d: -f1)"
cat_sign_line="$(grep -nF 'Sign-Artifact -Path $Cat' "$FINALIZER" | head -n 1 | cut -d: -f1)"
(( sys_sign_line < inf2cat_line && dll_sign_line < inf2cat_line && inf2cat_line < cat_sign_line )) ||
  fail "finalizer order must be PE sign -> Inf2Cat -> CAT sign"

cp -R "$OUT/package" "$CHECKER_FIXTURE"
printf 'parser-only smoke catalog\n' > "$CHECKER_FIXTURE/viogpu3d.cat"
checker_output="$(
  scripts/check-hvf-windows-viogpu3d-package.sh \
    --pci-device-id 1050 \
    --require-render-candidate \
    "$CHECKER_FIXTURE" 2>&1
)" || fail "staged INF failed render-candidate parser gate: $checker_output"
assert_contains "$checker_output" "package_capability=umd-registered" "checker output"
assert_contains "$checker_output" "render_candidate=true" "checker output"
assert_contains "$checker_output" "umd_active_copyfiles_payload_resolved=true" "checker output"

assert_fails_contains \
  "output path already exists" \
  bash scripts/stage-hvf-windows-viogpu3d-render-package.sh \
    --input-dir "$INPUT" \
    --source-inx "$SOURCE_INX" \
    --source-head fixture-head \
    --expected-source-head fixture-head \
    --expected-source-inx-sha256 "$source_hash" \
    --expected-input-manifest "$MANIFEST" \
    --out-dir "$OUT"

printf 'tampered\n' >> "$INPUT/viogpu_wgl_arm64.dll"
assert_fails_contains \
  "input hash mismatch for viogpu_wgl_arm64.dll" \
  bash scripts/stage-hvf-windows-viogpu3d-render-package.sh \
    --input-dir "$INPUT" \
    --source-inx "$SOURCE_INX" \
    --source-head fixture-head \
    --expected-source-head fixture-head \
    --expected-source-inx-sha256 "$source_hash" \
    --expected-input-manifest "$MANIFEST" \
    --out-dir "$STORE/tampered-out"

echo "PASS: viogpu3d render-package stage smoke ($STORE)"
