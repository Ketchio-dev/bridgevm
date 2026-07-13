#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

STORE="$(mktemp -d "/tmp/bridgevm-viogpu3d-build-kit.XXXXXX")"
SRC="$STORE/source"
OUT="$STORE/out"

mkdir -p "$SRC/viogpu/viogpu3d" "$SRC/viogpu/common"

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

cat > "$SRC/viogpu/viogpu3d/viogpu3d_arm64.inx" <<'EOF'
[Manufacturer]
%RedHat% = RedHat,NTarm64

[RedHat.NTarm64]
%VirtioGpu3D.DeviceDesc% = VioGpu3D_Inst, PCI\VEN_1AF4&DEV_1050&SUBSYS_11001AF4&REV_01
EOF

cat > "$SRC/viogpu/viogpu3d/viogpu3d.inx" <<'EOF'
; legacy decoy must lose to the ARM64-specific source contract
Device = Install, PCI\VEN_1AF4&DEV_10F7
EOF

cat > "$SRC/viogpu/viogpu3d/viogpu3d.vcxproj" <<'EOF'
<Project>
  <ItemGroup Label="ProjectConfigurations">
    <ProjectConfiguration Include="Win10 Release|ARM64">
      <Configuration>Win10 Release</Configuration>
      <Platform>ARM64</Platform>
    </ProjectConfiguration>
  </ItemGroup>
  <ItemGroup Condition="Exists('$(MESA_PREFIX)\bin\viogpu_d3d10.dll')">
    <FilesToPackage Include="$(MESA_PREFIX)\bin\viogpu_d3d10.dll" />
    <FilesToPackage Include="$(MESA_PREFIX)\bin\viogpu_wgl.dll" />
    <FilesToPackage Include="$(MESA_PREFIX)\bin\z.dll" />
  </ItemGroup>
</Project>
EOF

cat > "$SRC/viogpu/viogpu3d/BUILDING.md" <<'EOF'
# Building viogpu3d

Build Mesa with -Dgallium-drivers=virgl -Dgallium-d3d10umd=true.
EOF

cat > "$SRC/viogpu/common/virgl_hw.h" <<'EOF'
// virgl marker
EOF

cat > "$SRC/viogpu/viogpu.sln" <<'EOF'
Microsoft Visual Studio Solution File, Format Version 12.00
EOF

output="$(
  scripts/prepare-hvf-windows-viogpu3d-build-kit.sh \
    --source-dir "$SRC" \
    --repo https://example.invalid/kvm-guest-drivers-windows.git \
    --ref viogpu3d-test \
    --out-dir "$OUT" \
    --no-fetch 2>&1
)" || fail "build kit generation failed: $output"

assert_contains "$output" "BridgeVM viogpu3d external build kit" "build kit output"
assert_contains "$output" "protocol=virgl" "build kit output"
assert_contains "$output" "hwids=PCI\\VEN_1AF4&DEV_1050" "build kit output"
assert_contains "$output" "arm64_configuration_present=true" "build kit output"
assert_contains "$output" "mesa_prefix_required=true" "build kit output"
assert_contains "$output" "bridgevm_required_installed_host_protocol=virgl" "build kit output"
assert_contains "$output" "boot_runtime_selector=--gpu-trace-protocol virgl" "build kit output"
assert_contains "$output" "finalization_required=true" "build kit output"
assert_contains "$output" "render_candidate_verified=false" "build kit output"
assert_contains "$output" "boot_blocker=windows WDK finalization and repository render-candidate check required" "build kit output"

assert_file_contains "$OUT/source-report.txt" "source_ref=viogpu3d-test" "source report"
assert_file_contains "$OUT/source-report.txt" "inf_template=$SRC/viogpu/viogpu3d/viogpu3d_arm64.inx" "source report"
assert_file_contains "$OUT/source-report.txt" "builder_default_driver_ref=4c27e477e6560cea724d848b98149f03cb1f2083" "source report"
assert_file_contains "$OUT/source-report.txt" "builder_default_mesa_ref=cb531c440ff34a9c6334859dda0848132be49ec3" "source report"
assert_file_contains "$OUT/source-report.txt" "builder_host_arch=x64" "source report"
assert_file_contains "$OUT/source-report.txt" "builder_target_arch=arm64" "source report"
assert_file_contains "$OUT/source-report.txt" "builder_mesa_toolchain=clang-cl-cross" "source report"
assert_file_contains "$OUT/source-report.txt" "windows_finalization_order=InfVerif,PE-sign,Inf2Cat,CAT-sign,SignTool-verify" "source report"
assert_file_contains "$OUT/source-report.txt" "pre_finalization_manifest_required=true" "source report"
assert_file_contains "$OUT/source-report.txt" "finalizer_mutates_unsigned_input=false" "source report"
assert_file_contains "$OUT/README.txt" "VirGL/DEV_1050" "readme"
assert_file_contains "$OUT/README.txt" "package-finalized" "readme"
assert_file_contains "$OUT/README.txt" "--require-render-candidate" "readme"
assert_file_contains "$OUT/build-viogpu3d-arm64.ps1" "Set-StrictMode -Version Latest" "PowerShell builder"
assert_file_contains "$OUT/build-viogpu3d-arm64.ps1" "https://github.com/arehnman/virtio-win-mesa.git" "PowerShell builder"
assert_file_contains "$OUT/build-viogpu3d-arm64.ps1" 'MesaRef = "cb531c440ff34a9c6334859dda0848132be49ec3"' "PowerShell builder"
assert_file_contains "$OUT/build-viogpu3d-arm64.ps1" 'git" -Arguments @("-C", $MesaSrc, "apply", "--check", $MesaPatch)' "Mesa patch preflight"
assert_file_contains "$OUT/virtio-win-mesa-unbound-clear.patch" "clear_render_target may target a surface that is not currently bound" "Mesa unbound clear patch"
assert_file_contains "$OUT/build-viogpu3d-arm64.ps1" 'MesaSubmitTracePatch = Join-Path $PSScriptRoot "virtio-win-mesa-submit-trace.patch"' "Mesa submit trace preflight"
assert_file_contains "$OUT/virtio-win-mesa-submit-trace.patch" "BV-VIRGL-SUBMIT stage=%s" "Mesa submit trace patch"
assert_file_contains "$OUT/virtio-win-mesa-submit-trace.patch" "BV-D3D10-ENTRY %s" "Mesa D3D10 entry trace patch"
assert_file_contains "$OUT/build-viogpu3d-arm64.ps1" 'DriverRef = "4c27e477e6560cea724d848b98149f03cb1f2083"' "PowerShell builder"
assert_file_contains "$OUT/build-viogpu3d-arm64.ps1" "Invoke-NativeCommand" "PowerShell builder"
assert_file_contains "$OUT/build-viogpu3d-arm64.ps1" 'fetch --depth 1 origin $Ref' "pinned shallow fetch"
assert_file_contains "$OUT/build-viogpu3d-arm64.ps1" "pinned fetch failed after 8 attempts" "pinned fetch retry"
assert_file_contains "$OUT/build-viogpu3d-arm64.ps1" "Test-PinnedCommitPresent" "pinned fetch skip"
assert_file_contains "$OUT/build-viogpu3d-arm64.ps1" "build-mesa-arm64.ps1" "PowerShell builder"
assert_file_contains "$OUT/run-submit-trace-build.cmd" "Microsoft.VisualStudio.Component.VC.Tools.ARM64" "Windows submit trace runner"
assert_file_contains "$OUT/run-submit-trace-build.cmd" "bridgevm-viogpu3d-submit-trace-finalized.zip" "Windows submit trace runner"
assert_file_contains "$OUT/build-viogpu3d-arm64.ps1" '"/p:Configuration=Win11 Release"' "PowerShell builder"
assert_file_contains "$OUT/build-viogpu3d-arm64.ps1" '$env:MESA_PREFIX_ARM64 = $DriverMesaEmpty' "PowerShell builder"
assert_file_contains "$OUT/build-viogpu3d-arm64.ps1" '".\viogpu.sln", "/m"' "PowerShell builder"
assert_file_contains "$OUT/build-viogpu3d-arm64.ps1" '"/t:viogpu3d"' "PowerShell builder"
assert_file_contains "$OUT/build-viogpu3d-arm64.ps1" '"viogpu\viogpu3d\objfre_win10_arm64\arm64\viogpu3d.sys"' "PowerShell builder"
assert_file_contains "$OUT/build-viogpu3d-arm64.ps1" '$DriverProjectText.Replace($LinkNeedle, "$VirtioOut;$LinkNeedle")' "PowerShell builder"
assert_file_contains "$OUT/build-viogpu3d-arm64.ps1" "VIOGPU3D_PCI_DEVICE_ID=1050" "PowerShell builder"
assert_file_contains "$OUT/build-viogpu3d-arm64.ps1" "VIOGPU3D_BUILD_ID=arehnman-arm64-minimal-" "PowerShell builder"
assert_file_contains "$OUT/build-viogpu3d-arm64.ps1" "PreFinalizationManifest = \$PreFinalizationManifest" "PowerShell builder"
assert_file_contains "$OUT/build-viogpu3d-arm64.ps1" '& $Finalizer @FinalizeArgs' "PowerShell builder"
assert_file_contains "$OUT/finalize-viogpu3d-package.ps1" "InfVerif.exe" "PowerShell finalizer"
assert_file_contains "$OUT/finalize-viogpu3d-package.ps1" "Inf2Cat.exe" "PowerShell finalizer"
assert_file_contains "$OUT/finalize-viogpu3d-package.ps1" 'Assert-PinnedInputManifest' "PowerShell finalizer"
assert_file_contains "$OUT/finalize-viogpu3d-package.ps1" '[System.IO.Directory]::Move($workingPackageDir, $FinalizedDir)' "PowerShell finalizer"
assert_file_contains "$OUT/finalize-viogpu3d-test-package.ps1" 'New-SelfSignedCertificate' "PowerShell test finalizer"
assert_file_contains "$OUT/finalize-viogpu3d-test-package.ps1" '$finalizationSucceeded = $true' "PowerShell test finalizer"
assert_file_contains "$OUT/source-report.txt" 'windows_test_certificate_finalizer=' "source report"
assert_file_contains "$OUT/build-mesa-arm64.ps1" "-Dgallium-drivers=virgl" "Mesa ARM64 builder"
assert_file_contains "$OUT/build-mesa-arm64.ps1" "-Dvulkan-drivers=" "Mesa ARM64 builder"
assert_file_contains "$OUT/mesa-cross-arm64.ini" "arm64-pc-windows-msvc" "Mesa ARM64 cross file"
assert_file_contains "$OUT/viogpu3d-arehnman-arm64-minimal.inf" "UserModeDriverName" "minimal INF template"

if bash scripts/prepare-hvf-windows-viogpu3d-build-kit.sh \
  --source-dir "$SRC" \
  --ref viogpu3d-test \
  --out-dir "$OUT" \
  --no-fetch >/dev/null 2>&1; then
  fail "build kit unexpectedly reused an existing output directory"
fi

echo "PASS: viogpu3d build kit smoke ($STORE)"
