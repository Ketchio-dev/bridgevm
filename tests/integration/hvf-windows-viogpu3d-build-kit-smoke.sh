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

cat > "$SRC/viogpu/viogpu3d/viogpu3d.inx" <<'EOF'
[Manufacturer]
%RedHat% = RedHat,NTarm64

[RedHat.NTarm64]
%VirtioGpu3D.DeviceDesc% = VioGpu3D_Inst, PCI\VEN_1AF4&DEV_1050&SUBSYS_11001AF4&REV_01
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
assert_contains "$output" "boot_blocker=none" "build kit output"

assert_file_contains "$OUT/source-report.txt" "source_ref=viogpu3d-test" "source report"
assert_file_contains "$OUT/README.txt" "VirGL/DEV_1050" "readme"
assert_file_contains "$OUT/build-viogpu3d-arm64.ps1" "-Dgallium-drivers=virgl" "PowerShell builder"
assert_file_contains "$OUT/build-viogpu3d-arm64.ps1" "msbuild .\\viogpu.sln" "PowerShell builder"
assert_file_contains "$OUT/build-viogpu3d-arm64.ps1" "VIOGPU3D_PCI_DEVICE_ID=1050" "PowerShell builder"

echo "PASS: viogpu3d build kit smoke ($STORE)"
