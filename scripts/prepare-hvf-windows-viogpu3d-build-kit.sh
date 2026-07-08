#!/usr/bin/env bash
# Prepare a reproducible external Windows ARM64 build kit for PR #943 viogpu3d.
set -euo pipefail

SOURCE_REPO="${SOURCE_REPO:-https://github.com/max8rr8/kvm-guest-drivers-windows.git}"
SOURCE_REF="${SOURCE_REF:-viogpu3d}"
SOURCE_DIR="${SOURCE_DIR:-$HOME/BridgeVM/viogpu3d-pr943}"
OUT_DIR="${OUT_DIR:-}"
FETCH_SOURCE="${FETCH_SOURCE:-1}"

usage() {
  cat >&2 <<'EOF'
usage: scripts/prepare-hvf-windows-viogpu3d-build-kit.sh [--source-dir DIR] [--repo URL] [--ref REF] [--out-dir DIR] [--no-fetch]

Options:
  --source-dir DIR  Local viogpu3d source checkout. Default:
                    $HOME/BridgeVM/viogpu3d-pr943.
  --repo URL        Source repo to clone/fetch when --no-fetch is not used.
                    Default: max8rr8/kvm-guest-drivers-windows.git.
  --ref REF         Source branch/ref. Default: viogpu3d.
  --out-dir DIR     Output directory for source-report.txt, README.txt, and
                    build-viogpu3d-arm64.ps1. Default:
                    /tmp/bridgevm-viogpu3d-build-kit.<pid>.
  --no-fetch        Do not clone/fetch; inspect --source-dir as-is.

The generated PowerShell script is meant to run on an external Windows ARM64
developer machine with Visual Studio, WDK, Git, Meson, and Ninja installed. PR
#943 viogpu3d is a VirGL/DEV_1050 path; BridgeVM boots that package through the
CGL-backed VirGL runtime selected with --gpu-trace-protocol virgl.
EOF
}

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

sanitize_ref() {
  printf '%s\n' "$1" | sed 's#[^A-Za-z0-9._/-]#_#g'
}

clone_or_fetch_source() {
  [[ "$FETCH_SOURCE" == "1" ]] || return 0
  if [[ -d "$SOURCE_DIR/.git" ]]; then
    git -C "$SOURCE_DIR" fetch --depth 1 origin "$(sanitize_ref "$SOURCE_REF")"
    git -C "$SOURCE_DIR" checkout -q FETCH_HEAD
  else
    mkdir -p "$(dirname "$SOURCE_DIR")"
    git clone --depth 1 --branch "$SOURCE_REF" "$SOURCE_REPO" "$SOURCE_DIR"
  fi
}

extract_hwids() {
  local inf="$1"
  local ids=""
  local match
  local id
  while IFS= read -r match; do
    [[ -n "$match" ]] || continue
    id="$(printf '%s\n' "$match" | grep -Eio 'DEV_[0-9A-F]{4}' | head -n 1 | cut -d_ -f2 | tr '[:lower:]' '[:upper:]')"
    case " $ids " in
      *" $id "*) ;;
      *) ids="${ids:+$ids }$id" ;;
    esac
  done < <(
    LC_ALL=C grep -Eio 'VEN_1AF4[^[:space:],;]*DEV_(1050|10F7)|DEV_(1050|10F7)[^[:space:],;]*VEN_1AF4' "$inf" 2>/dev/null || true
  )

  local out=""
  local sep=""
  for id in $ids; do
    out="${out}${sep}PCI\\VEN_1AF4&DEV_$id"
    sep=","
  done
  printf '%s\n' "$out"
}

detect_source_protocol() {
  local source_dir="$1"
  local venus_hit=0
  local virgl_hit=0
  if grep -rqiE 'venus|vulkan|capset[^[:alnum:]]*4' "$source_dir/viogpu/viogpu3d" "$source_dir/viogpu/common" 2>/dev/null; then
    venus_hit=1
  fi
  if grep -rqiE 'virgl|d3d10|d3d10umd|wgl|opengl|gallium' "$source_dir/viogpu/viogpu3d" "$source_dir/viogpu/common" 2>/dev/null; then
    virgl_hit=1
  fi
  if (( venus_hit == 1 && virgl_hit == 0 )); then
    printf 'venus\n'
  elif (( virgl_hit == 1 && venus_hit == 0 )); then
    printf 'virgl\n'
  elif (( venus_hit == 1 && virgl_hit == 1 )); then
    printf 'mixed\n'
  else
    printf 'unknown\n'
  fi
}

write_powershell_builder() {
  local path="$1"
  cat > "$path" <<'EOF'
param(
  [string]$WorkDir = "$env:USERPROFILE\BridgeVM-viogpu3d-build",
  [string]$MesaRepo = "https://gitlab.freedesktop.org/mesa/mesa.git",
  [string]$DriverRepo = "https://github.com/max8rr8/kvm-guest-drivers-windows.git",
  [string]$DriverRef = "viogpu3d",
  [string]$MesaPrefix = "",
  [string]$OutputDir = "",
  [string]$CertificatePfx = "",
  [string]$CertificatePassword = "",
  [switch]$SkipMesa,
  [switch]$SkipDriverFetch
)

$ErrorActionPreference = "Stop"

function Require-Command($Name) {
  if (-not (Get-Command $Name -ErrorAction SilentlyContinue)) {
    throw "Required command not found in PATH: $Name"
  }
}

Require-Command git
Require-Command msbuild
if (-not $SkipMesa) {
  Require-Command meson
  Require-Command ninja
}

$WorkDir = [System.IO.Path]::GetFullPath($WorkDir)
if ($MesaPrefix -eq "") { $MesaPrefix = Join-Path $WorkDir "mesa_prefix" }
if ($OutputDir -eq "") { $OutputDir = Join-Path $WorkDir "bridgevm-viogpu3d-arm64-package" }
$MesaPrefix = [System.IO.Path]::GetFullPath($MesaPrefix)
$OutputDir = [System.IO.Path]::GetFullPath($OutputDir)
$MesaSrc = Join-Path $WorkDir "mesa"
$MesaBuild = Join-Path $MesaSrc "build-arm64"
$DriverSrc = Join-Path $WorkDir "kvm-guest-drivers-windows"

New-Item -ItemType Directory -Force -Path $WorkDir, $MesaPrefix, $OutputDir | Out-Null

if (-not $SkipMesa) {
  if (-not (Test-Path (Join-Path $MesaSrc ".git"))) {
    git clone $MesaRepo $MesaSrc
  }
  Push-Location $MesaSrc
  if (-not (Test-Path $MesaBuild)) {
    meson setup $MesaBuild --prefix=$MesaPrefix -Dgallium-drivers=virgl -Dgallium-d3d10umd=true -Dgallium-wgl-dll-name=viogpu_wgl -Dgallium-d3d10-dll-name=viogpu_d3d10 -Db_vscrt=mt
  }
  meson install -C $MesaBuild
  Pop-Location
}

if (-not $SkipDriverFetch) {
  if (-not (Test-Path (Join-Path $DriverSrc ".git"))) {
    git clone --branch $DriverRef $DriverRepo $DriverSrc
  } else {
    git -C $DriverSrc fetch origin $DriverRef
    git -C $DriverSrc checkout FETCH_HEAD
  }
}

$env:MESA_PREFIX = $MesaPrefix
Push-Location (Join-Path $DriverSrc "viogpu")
msbuild .\viogpu.sln /m /p:Configuration="Win10 Release" /p:Platform=ARM64 /p:SignMode=Off
Pop-Location

$PackageDir = Join-Path $DriverSrc "viogpu\viogpu3d\objfre_win10_arm64\arm64\viogpu3d"
if (-not (Test-Path $PackageDir)) {
  throw "Expected viogpu3d package directory missing: $PackageDir"
}

Copy-Item -Force (Join-Path $PackageDir "*") $OutputDir

if ($CertificatePfx -ne "") {
  Require-Command signtool
  $Cat = Join-Path $OutputDir "viogpu3d.cat"
  if (-not (Test-Path $Cat)) { throw "Catalog missing before signing: $Cat" }
  if ($CertificatePassword -ne "") {
    signtool sign /fd SHA256 /f $CertificatePfx /p $CertificatePassword $Cat
  } else {
    signtool sign /fd SHA256 /f $CertificatePfx $Cat
  }
}

$Head = (git -C $DriverSrc rev-parse HEAD).Trim()
$Signing = if ($CertificatePfx -ne "") { $CertificatePfx } else { "<unsigned-or-external-test-signing-required>" }
@"
VIOGPU3D_SOURCE_REPO=$DriverRepo
VIOGPU3D_SOURCE_REF=$Head
VIOGPU3D_BUILD_ID=$(Get-Date -Format o)
VIOGPU3D_SIGNING_CERT=$Signing
VIOGPU3D_PROTOCOL=virgl
VIOGPU3D_PCI_DEVICE_ID=1050
"@ | Set-Content -Encoding ascii (Join-Path $OutputDir "bridgevm-package-provenance.env")

Write-Host "BridgeVM viogpu3d ARM64 package staged at $OutputDir"
Write-Host "Host note: PR #943 is VirGL/DEV_1050; use --gpu-trace-protocol virgl for the matching BridgeVM runtime."
EOF
}

write_readme() {
  local path="$1"
  cat > "$path" <<'EOF'
BridgeVM Windows ARM64 viogpu3d build kit

This kit targets virtio-win PR #943 / max8rr8 viogpu3d. It is a VirGL/DEV_1050
driver package that binds PCI\VEN_1AF4&DEV_1050 in the source INF. Build it on an
external Windows ARM64 development machine with Visual Studio, WDK, Git, Meson,
and Ninja installed.

Run from a Visual Studio developer PowerShell:

  powershell -ExecutionPolicy Bypass -File .\build-viogpu3d-arm64.ps1

After it produces bridgevm-viogpu3d-arm64-package, copy that package directory
back to the Mac and run:

  scripts/check-hvf-windows-viogpu3d-package.sh \
    --manifest /tmp/viogpu3d-package-manifest.txt \
    /path/to/package

The checker auto-loads bridgevm-package-provenance.env from the package
directory, including the PR source commit, build id, signing note, protocol, and
expected DEV_1050 HWID.

BridgeVM defaults to the Venus runtime for installed P3 boots. A PR #943 VirGL
package requires the VirGL runtime selector:

  --gpu-trace-protocol virgl
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --source-dir)
      [[ $# -ge 2 ]] || { usage; exit 2; }
      SOURCE_DIR="$2"
      shift 2
      ;;
    --repo)
      [[ $# -ge 2 ]] || { usage; exit 2; }
      SOURCE_REPO="$2"
      shift 2
      ;;
    --ref)
      [[ $# -ge 2 ]] || { usage; exit 2; }
      SOURCE_REF="$2"
      shift 2
      ;;
    --out-dir)
      [[ $# -ge 2 ]] || { usage; exit 2; }
      OUT_DIR="$2"
      shift 2
      ;;
    --no-fetch)
      FETCH_SOURCE="0"
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      usage
      exit 2
      ;;
  esac
done

if [[ -z "$OUT_DIR" ]]; then
  OUT_DIR="/tmp/bridgevm-viogpu3d-build-kit.$$"
fi

clone_or_fetch_source

INX="$SOURCE_DIR/viogpu/viogpu3d/viogpu3d.inx"
VCXPROJ="$SOURCE_DIR/viogpu/viogpu3d/viogpu3d.vcxproj"
BUILDING="$SOURCE_DIR/viogpu/viogpu3d/BUILDING.md"
SLN="$SOURCE_DIR/viogpu/viogpu.sln"
[[ -f "$INX" ]] || fail "missing viogpu3d INF template: $INX"
[[ -f "$VCXPROJ" ]] || fail "missing viogpu3d vcxproj: $VCXPROJ"
[[ -f "$BUILDING" ]] || fail "missing viogpu3d BUILDING.md: $BUILDING"
[[ -f "$SLN" ]] || fail "missing viogpu solution: $SLN"

mkdir -p "$OUT_DIR"

git_head="<not-a-git-checkout>"
if [[ -d "$SOURCE_DIR/.git" ]]; then
  git_head="$(git -C "$SOURCE_DIR" rev-parse HEAD)"
fi
protocol="$(detect_source_protocol "$SOURCE_DIR")"
hwids="$(extract_hwids "$INX")"
[[ -n "$hwids" ]] || fail "source INF does not advertise DEV_1050 or DEV_10F7: $INX"
if grep -Fq 'Win10 Release|ARM64' "$VCXPROJ"; then
  arm64_config="true"
else
  arm64_config="false"
fi
if grep -Fq 'MESA_PREFIX' "$VCXPROJ" && grep -Fq 'viogpu_d3d10.dll' "$VCXPROJ"; then
  mesa_prefix_required="true"
else
  mesa_prefix_required="false"
fi

write_powershell_builder "$OUT_DIR/build-viogpu3d-arm64.ps1"
write_readme "$OUT_DIR/README.txt"

REPORT="$OUT_DIR/source-report.txt"
{
  printf 'BridgeVM viogpu3d external build kit\n'
  printf 'generated_utc=%s\n' "$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
  printf 'source_repo=%s\n' "$SOURCE_REPO"
  printf 'source_ref=%s\n' "$SOURCE_REF"
  printf 'source_dir=%s\n' "$SOURCE_DIR"
  printf 'source_head=%s\n' "$git_head"
  printf 'driver_solution=%s\n' "$SLN"
  printf 'driver_project=%s\n' "$VCXPROJ"
  printf 'inf_template=%s\n' "$INX"
  printf 'protocol=%s\n' "$protocol"
  printf 'hwids=%s\n' "$hwids"
  printf 'arm64_configuration_present=%s\n' "$arm64_config"
  printf 'mesa_prefix_required=%s\n' "$mesa_prefix_required"
  printf 'required_mesa_dlls=viogpu_d3d10.dll,viogpu_wgl.dll,z.dll\n'
  printf 'windows_build_script=%s\n' "$OUT_DIR/build-viogpu3d-arm64.ps1"
  printf 'readme=%s\n' "$OUT_DIR/README.txt"
  printf 'bridgevm_default_installed_host_protocol=venus\n'
  printf 'bridgevm_required_installed_host_protocol=%s\n' "$protocol"
  if [[ "$protocol" == "virgl" ]]; then
    printf 'boot_runtime_selector=--gpu-trace-protocol virgl\n'
    printf 'boot_blocker=none\n'
  elif [[ "$protocol" == "venus" ]]; then
    printf 'boot_runtime_selector=--gpu-trace-protocol venus\n'
    printf 'boot_blocker=none\n'
  else
    printf 'boot_runtime_selector=manual-protocol-audit-required\n'
    printf 'boot_blocker=source protocol %s is not boot-selectable without a manual audit\n' "$protocol"
  fi
} > "$REPORT"

cat "$REPORT"
