#!/usr/bin/env bash
# Prepare a reproducible external Windows ARM64 build/finalization kit for the
# arehnman akre viogpu3d source profile.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SOURCE_REPO="${SOURCE_REPO:-https://github.com/arehnman/kvm-guest-drivers-windows.git}"
SOURCE_REF="${SOURCE_REF:-akre}"
SOURCE_DIR="${SOURCE_DIR:-$HOME/BridgeVM/viogpu3d-arehnman}"
OUT_DIR="${OUT_DIR:-}"
FETCH_SOURCE="${FETCH_SOURCE:-1}"

usage() {
  cat >&2 <<'EOF'
usage: scripts/prepare-hvf-windows-viogpu3d-build-kit.sh [--source-dir DIR] [--repo URL] [--ref REF] [--out-dir DIR] [--no-fetch]

Options:
  --source-dir DIR  Local viogpu3d source checkout. Default:
                    $HOME/BridgeVM/viogpu3d-arehnman.
  --repo URL        Source repo to clone/fetch when --no-fetch is not used.
                    Default: arehnman/kvm-guest-drivers-windows.git.
  --ref REF         Source branch/ref. Default: akre.
  --out-dir DIR     Output directory for source-report.txt, README.txt, and
                    build-viogpu3d-arm64.ps1, and the WDK finalizer. Default:
                    /tmp/bridgevm-viogpu3d-build-kit.<pid>.
  --no-fetch        Do not clone/fetch; inspect --source-dir as-is.

The generated PowerShell script is meant to run from an x64 Visual Studio
developer PowerShell with WDK, ARM64 C++ tools, LLVM, Git, Meson, Ninja, and
                    win_flex/win_bison on PATH.
It reproduces the proven clang-cl ARM64 cross-build rather than attempting to
execute ARM64 Mesa tools on the build host. The akre source is a VirGL/DEV_1050
path; BridgeVM selects its CGL-backed runtime with --gpu-trace-protocol virgl.
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
  [string]$MesaRepo = "https://github.com/arehnman/virtio-win-mesa.git",
  [string]$MesaRef = "cb531c440ff34a9c6334859dda0848132be49ec3",
  [string]$DriverRepo = "https://github.com/arehnman/kvm-guest-drivers-windows.git",
  [string]$DriverRef = "4c27e477e6560cea724d848b98149f03cb1f2083",
  [Alias("MesaPrefix")]
  [string]$MesaPrefixArm64 = "",
  [string]$OutputDir = "",
  [string]$CertificatePfx = "",
  [string]$CertificatePassword = "",
  [switch]$SkipMesa,
  [switch]$SkipDriverFetch,
  [string]$DriverSysPath = ""
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

function Require-Command($Name) {
  if (-not (Get-Command $Name -ErrorAction SilentlyContinue)) {
    throw "Required command not found in PATH: $Name"
  }
}

function Invoke-NativeCommand {
  param(
    [Parameter(Mandatory = $true)][string]$CommandName,
    [Parameter(Mandatory = $true)][string[]]$Arguments,
    [Parameter(Mandatory = $true)][string]$Label
  )
  $output = & $CommandName @Arguments
  if ($LASTEXITCODE -ne 0) {
    throw "$Label failed with exit code $LASTEXITCODE"
  }
  return $output
}

function Assert-CleanGitCheckout {
  param(
    [Parameter(Mandatory = $true)][string]$Path,
    [Parameter(Mandatory = $true)][string]$Label
  )
  $dirty = @(Invoke-NativeCommand -CommandName "git" -Arguments @("-C", $Path, "status", "--porcelain") -Label "$Label status")
  if ($dirty.Count -ne 0) {
    throw "$Label checkout is dirty; use a clean work directory: $Path"
  }
}

function Test-PinnedCommitPresent {
  param(
    [Parameter(Mandatory = $true)][string]$Path,
    [Parameter(Mandatory = $true)][string]$Ref
  )
  & git -C $Path rev-parse --verify --quiet "${Ref}^{commit}" *> $null
  return ($LASTEXITCODE -eq 0)
}

function Checkout-PinnedGitRef {
  param(
    [Parameter(Mandatory = $true)][string]$Path,
    [Parameter(Mandatory = $true)][string]$Repo,
    [Parameter(Mandatory = $true)][string]$Ref,
    [Parameter(Mandatory = $true)][string]$Label,
    [switch]$SkipFetch
  )
  # A 40-hex pinned commit is content-addressed: once it is present locally a
  # fetch adds nothing and only re-exposes the build to transient guest-NAT
  # transport resets, and a fresh checkout needs only that single commit.
  # Branch/tag refs keep the legacy full clone + prune fetch behavior.
  $isPinnedSha = ($Ref -match '^[0-9a-fA-F]{40}$')
  if (-not (Test-Path (Join-Path $Path ".git"))) {
    if ($SkipFetch) {
      throw "$Label checkout is missing while fetch is disabled: $Path"
    }
    if ($isPinnedSha) {
      New-Item -ItemType Directory -Force -Path $Path | Out-Null
      $null = Invoke-NativeCommand -CommandName "git" -Arguments @("init", $Path) -Label "$Label init"
      $null = Invoke-NativeCommand -CommandName "git" -Arguments @("-C", $Path, "remote", "add", "origin", $Repo) -Label "$Label remote"
    } else {
      $null = Invoke-NativeCommand -CommandName "git" -Arguments @("clone", $Repo, $Path) -Label "$Label clone"
    }
  }
  Assert-CleanGitCheckout -Path $Path -Label $Label
  if (-not $SkipFetch) {
    if ($isPinnedSha) {
      if (-not (Test-PinnedCommitPresent -Path $Path -Ref $Ref)) {
        $null = Invoke-NativeCommand -CommandName "git" -Arguments @("-C", $Path, "config", "http.lowSpeedLimit", "1000") -Label "$Label config low-speed limit"
        $null = Invoke-NativeCommand -CommandName "git" -Arguments @("-C", $Path, "config", "http.lowSpeedTime", "60") -Label "$Label config low-speed time"
        $fetched = $false
        for ($attempt = 1; $attempt -le 8; $attempt++) {
          # Plain invocation: 2>&1 under $ErrorActionPreference=Stop converts
          # git's stderr progress into throwing NativeCommandError records on
          # Windows PowerShell 5.1. Let stderr flow to the process stream.
          & git -C $Path fetch --depth 1 origin $Ref
          if ($LASTEXITCODE -eq 0) {
            $fetched = $true
            break
          }
          Write-Host "$Label pinned fetch attempt $attempt failed (exit code $LASTEXITCODE); retrying"
          Start-Sleep -Seconds ([Math]::Min(15 * $attempt, 60))
        }
        if (-not $fetched) {
          throw "$Label pinned fetch failed after 8 attempts"
        }
      }
    } else {
      $null = Invoke-NativeCommand -CommandName "git" -Arguments @("-C", $Path, "fetch", "origin", "--prune") -Label "$Label fetch"
    }
  }
  $null = Invoke-NativeCommand -CommandName "git" -Arguments @("-C", $Path, "checkout", "--detach", $Ref) -Label "$Label checkout"
  $headOutput = @(Invoke-NativeCommand -CommandName "git" -Arguments @("-C", $Path, "rev-parse", "HEAD") -Label "$Label HEAD")
  $expectedOutput = @(Invoke-NativeCommand -CommandName "git" -Arguments @("-C", $Path, "rev-parse", "${Ref}^{commit}") -Label "$Label pinned ref")
  $head = $headOutput[-1].Trim()
  $expected = $expectedOutput[-1].Trim()
  if ($head -cne $expected) {
    throw "$Label did not resolve to the pinned ref: expected $expected, got $head"
  }
  return $head
}

Require-Command git
Require-Command msbuild
if (-not $SkipMesa) {
  foreach ($Tool in @("cl.exe", "clang-cl.exe", "lld-link.exe", "llvm-lib.exe", "llvm-rc.exe", "llvm-strip.exe", "meson.exe", "ninja.exe")) {
    Require-Command $Tool
  }
}

$WorkDir = [System.IO.Path]::GetFullPath($WorkDir)
if ($MesaPrefixArm64 -eq "") { $MesaPrefixArm64 = Join-Path $WorkDir "mesa_prefix_arm64" }
if ($OutputDir -eq "") { $OutputDir = Join-Path $WorkDir "bridgevm-viogpu3d-arm64-package" }
$MesaPrefixArm64 = [System.IO.Path]::GetFullPath($MesaPrefixArm64)
$OutputDir = [System.IO.Path]::GetFullPath($OutputDir)
$MesaSrc = Join-Path $WorkDir "mesa"
$DriverSrc = Join-Path $WorkDir "kvm-guest-drivers-windows"

New-Item -ItemType Directory -Force -Path $WorkDir, $OutputDir | Out-Null
if (@(Get-ChildItem -LiteralPath $OutputDir -Force).Count -ne 0) {
  throw "OutputDir must be empty to prevent stale package reuse: $OutputDir"
}

if ($DriverSysPath -ne "") {
  # UMD-only rebuild mode: reuse an already-built pinned KMD payload instead of
  # requiring the WDK Visual Studio MSBuild toolsets (MSB8020) on this builder.
  # The provenance line records the reuse explicitly.
  $DriverSys = [System.IO.Path]::GetFullPath($DriverSysPath)
  if (-not (Test-Path -LiteralPath $DriverSys -PathType Leaf)) {
    throw "DriverSysPath does not exist: $DriverSys"
  }
  $DriverHead = "$DriverRef(prebuilt-sys)"
} else {
$DriverHead = Checkout-PinnedGitRef -Path $DriverSrc -Repo $DriverRepo -Ref $DriverRef -Label "viogpu3d driver" -SkipFetch:$SkipDriverFetch
# The pinned solution intentionally maps viogpu3d's Win11 ARM64 entry to its
# Win10 Release project configuration while its VirtIO dependency stays Win11.
$DriverSys = Join-Path $DriverSrc "viogpu\viogpu3d\objfre_win10_arm64\arm64\viogpu3d.sys"
if (Test-Path -LiteralPath $DriverSys) {
  throw "Driver output already exists; use a clean WorkDir: $DriverSys"
}

$DriverMesaEmpty = Join-Path $WorkDir "mesa_empty_for_driver"
if (Test-Path -LiteralPath $DriverMesaEmpty) {
  if (@(Get-ChildItem -LiteralPath $DriverMesaEmpty -Force).Count -ne 0) {
    throw "Driver-only Mesa sentinel must be empty: $DriverMesaEmpty"
  }
} else {
  New-Item -ItemType Directory -Path $DriverMesaEmpty | Out-Null
}
$env:MESA_PREFIX_ARM64 = $DriverMesaEmpty

# The WDK normalizes OutDir to an absolute path for ARM64. Prepend the absolute
# VirtIO output directory so the proven viogpu3d cross-link can find virtiolib.
$VirtioOut = Join-Path $DriverSrc "VirtIO\objfre_win11_arm64\arm64"
$DriverProject = Join-Path $DriverSrc "viogpu\viogpu3d\viogpu3d.vcxproj"
$DriverProjectText = Get-Content -LiteralPath $DriverProject -Raw
$LinkNeedle = '..\..\VirtIO\$(OutDir)'
if (-not $DriverProjectText.Contains($LinkNeedle)) {
  throw "Pinned viogpu3d link-path contract is missing: $LinkNeedle"
}
$DriverProjectText = $DriverProjectText.Replace($LinkNeedle, "$VirtioOut;$LinkNeedle")
$Utf8NoBom = New-Object System.Text.UTF8Encoding($false)
[System.IO.File]::WriteAllText($DriverProject, $DriverProjectText, $Utf8NoBom)

Push-Location (Join-Path $DriverSrc "viogpu")
try {
  $null = Invoke-NativeCommand -CommandName "msbuild" -Arguments @(
    ".\viogpu.sln", "/m", "/t:viogpu3d", "/p:Configuration=Win11 Release", "/p:Platform=ARM64", "/p:SignMode=Off", "/v:minimal", "/nologo"
  ) -Label "viogpu3d ARM64 build"
} finally {
  Pop-Location
}

if (-not (Test-Path -LiteralPath $DriverSys -PathType Leaf)) {
  throw "Expected viogpu3d ARM64 miniport is missing after build: $DriverSys"
}
}

if (-not $SkipMesa) {
  $MesaHead = Checkout-PinnedGitRef -Path $MesaSrc -Repo $MesaRepo -Ref $MesaRef -Label "Mesa"
  $MesaPatch = Join-Path $PSScriptRoot "virtio-win-mesa-unbound-clear.patch"
  if (-not (Test-Path -LiteralPath $MesaPatch -PathType Leaf)) {
    throw "BridgeVM Mesa unbound-clear patch is missing: $MesaPatch"
  }
  $null = Invoke-NativeCommand -CommandName "git" -Arguments @("-C", $MesaSrc, "apply", "--check", $MesaPatch) -Label "Mesa unbound-clear patch check"
  $null = Invoke-NativeCommand -CommandName "git" -Arguments @("-C", $MesaSrc, "apply", $MesaPatch) -Label "Mesa unbound-clear patch"
  $MesaSubmitTracePatch = Join-Path $PSScriptRoot "virtio-win-mesa-submit-trace.patch"
  if (-not (Test-Path -LiteralPath $MesaSubmitTracePatch -PathType Leaf)) {
    throw "BridgeVM Mesa submit-trace patch is missing: $MesaSubmitTracePatch"
  }
  $null = Invoke-NativeCommand -CommandName "git" -Arguments @("-C", $MesaSrc, "apply", "--check", $MesaSubmitTracePatch) -Label "Mesa submit-trace patch check"
  $null = Invoke-NativeCommand -CommandName "git" -Arguments @("-C", $MesaSrc, "apply", $MesaSubmitTracePatch) -Label "Mesa submit-trace patch"
  $WdkIncludeRoot = Join-Path ${env:ProgramFiles(x86)} "Windows Kits\10\Include"
  $WdkIncludeVersion = Get-ChildItem -LiteralPath $WdkIncludeRoot -Directory |
    Sort-Object Name -Descending |
    Where-Object {
      (Test-Path -LiteralPath (Join-Path $_.FullName "um\d3d10umddi.h")) -and
      (Test-Path -LiteralPath (Join-Path $_.FullName "shared\d3dkmddi.h"))
    } |
    Select-Object -First 1
  if ($null -eq $WdkIncludeVersion) {
    throw "WDK d3d10umddi.h/d3dkmddi.h headers are missing under $WdkIncludeRoot"
  }
  $env:INCLUDE = "$(Join-Path $WdkIncludeVersion.FullName 'um');$(Join-Path $WdkIncludeVersion.FullName 'shared');$env:INCLUDE"
  $MesaBuilder = Join-Path $PSScriptRoot "build-mesa-arm64.ps1"
  $MesaCrossFile = Join-Path $PSScriptRoot "mesa-cross-arm64.ini"
  if (-not (Test-Path -LiteralPath $MesaBuilder -PathType Leaf)) {
    throw "BridgeVM Mesa ARM64 builder is missing: $MesaBuilder"
  }
  & $MesaBuilder -MesaSrc $MesaSrc -Prefix $MesaPrefixArm64 -CrossFileBase $MesaCrossFile
} else {
  if (-not (Test-Path -LiteralPath $MesaPrefixArm64 -PathType Container)) {
    throw "SkipMesa requires an existing MesaPrefixArm64: $MesaPrefixArm64"
  }
  $MesaHead = "<external-prefix>"
}

$TemplateInf = Join-Path $PSScriptRoot "viogpu3d-arehnman-arm64-minimal.inf"
if (-not (Test-Path $TemplateInf)) { throw "BridgeVM minimal render INF missing: $TemplateInf" }
$PayloadMap = @(
  @{ Source = $DriverSys; Destination = "viogpu3d.sys" },
  @{ Source = (Join-Path $MesaPrefixArm64 "bin\viogpu_d3d10.dll"); Destination = "viogpu_d3d10_arm64.dll" },
  @{ Source = (Join-Path $MesaPrefixArm64 "bin\viogpu_wgl.dll"); Destination = "viogpu_wgl_arm64.dll" },
  @{ Source = (Join-Path $MesaPrefixArm64 "bin\opengl32.dll"); Destination = "opengl32_arm64.dll" },
  @{ Source = (Join-Path $MesaPrefixArm64 "bin\libEGL.dll"); Destination = "libEGL_arm64.dll" },
  @{ Source = (Join-Path $MesaPrefixArm64 "bin\libGLESv2.dll"); Destination = "libGLESv2_arm64.dll" }
)
foreach ($Payload in $PayloadMap) {
  if (-not (Test-Path -LiteralPath $Payload.Source -PathType Leaf)) {
    throw "Required ARM64 render payload is missing: $($Payload.Source)"
  }
  Copy-Item -LiteralPath $Payload.Source -Destination (Join-Path $OutputDir $Payload.Destination)
}
Copy-Item -Force $TemplateInf (Join-Path $OutputDir "viogpu3d.inf")

$Signing = "<pending-windows-wdk-finalization>"
@"
VIOGPU3D_SOURCE_REPO=$DriverRepo
VIOGPU3D_SOURCE_REF=driver@$DriverHead mesa@$MesaHead
VIOGPU3D_BUILD_ID=arehnman-arm64-minimal-$(Get-Date -Format yyyyMMddTHHmmssK)
VIOGPU3D_SIGNING_CERT=$Signing
VIOGPU3D_PROTOCOL=virgl
VIOGPU3D_PCI_DEVICE_ID=1050
"@ | Set-Content -Encoding ascii (Join-Path $OutputDir "bridgevm-package-provenance.env")

$PreFinalizationManifest = Join-Path `
  (Split-Path -Parent $OutputDir) `
  ((Split-Path -Leaf $OutputDir) + "-pre-finalization.sha256")
if (Test-Path -LiteralPath $PreFinalizationManifest) {
  throw "Pre-finalization manifest already exists; use a clean WorkDir: $PreFinalizationManifest"
}
$ExpectedStageNames = @(
  "bridgevm-package-provenance.env",
  "viogpu3d.inf",
  "viogpu3d.sys",
  "libEGL_arm64.dll",
  "libGLESv2_arm64.dll",
  "opengl32_arm64.dll",
  "viogpu_d3d10_arm64.dll",
  "viogpu_wgl_arm64.dll"
)
$ManifestLines = @()
foreach ($Name in @($ExpectedStageNames | Sort-Object)) {
  $Path = Join-Path $OutputDir $Name
  $Hash = (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
  $ManifestLines += "$Hash  $Name"
}
$ManifestLines | Set-Content -LiteralPath $PreFinalizationManifest -Encoding Ascii

if ($CertificatePfx -ne "") {
  $Finalizer = Join-Path $PSScriptRoot "finalize-viogpu3d-package.ps1"
  if (-not (Test-Path $Finalizer)) { throw "BridgeVM WDK finalizer missing: $Finalizer" }
  $FinalizeArgs = @{
    PackageDir = $OutputDir
    PreFinalizationManifest = $PreFinalizationManifest
    CertificatePfx = $CertificatePfx
    Profile = "arehnman-arm64-minimal"
  }
  if ($CertificatePassword -ne "") { $FinalizeArgs.CertificatePassword = $CertificatePassword }
  & $Finalizer @FinalizeArgs
} else {
  Write-Warning "Package is unsigned and has no catalog. Re-run with -CertificatePfx or invoke finalize-viogpu3d-package.ps1 with -PreFinalizationManifest before injection."
}

Write-Host "BridgeVM viogpu3d ARM64 unsigned package staged at $OutputDir"
Write-Host "Pre-finalization manifest: $PreFinalizationManifest"
Write-Host "Host note: akre ARM64 is VirGL/DEV_1050; use --gpu-trace-protocol virgl for the matching BridgeVM runtime."
EOF
}

write_readme() {
  local path="$1"
  cat > "$path" <<'EOF'
BridgeVM Windows ARM64 viogpu3d build kit

This kit targets the ARM64-capable arehnman/akre viogpu3d source. It is a
VirGL/DEV_1050 driver package that binds PCI\VEN_1AF4&DEV_1050 in the source INF.
Build and finalize it from an x64 Visual Studio developer PowerShell with WDK,
the ARM64 C++ tools, LLVM (clang-cl/lld-link/llvm-lib), Git, Meson, Ninja, and
win_flex/win_bison (winflexbison) on PATH; a winflexbison directory shipped
beside the kit is added to PATH automatically by run-submit-trace-build.cmd.
The builder cross-compiles ARM64 payloads; it does not execute them on the host.

The generated builder defaults to the exact driver and modified Mesa revisions
already represented by the audited CI payload:

  driver 4c27e477e6560cea724d848b98149f03cb1f2083
  mesa   cb531c440ff34a9c6334859dda0848132be49ec3

Run from a Visual Studio developer PowerShell:

  powershell -ExecutionPolicy Bypass -File .\build-viogpu3d-arm64.ps1 `
    -CertificatePfx C:\path\BridgeVM-Test.pfx

Without -CertificatePfx the builder creates an exact unsigned staging package
and a sibling *-pre-finalization.sha256 manifest. Finalize that package before
injection. On an elevated disposable Windows test VM, the bundled test wrapper
generates and trusts a process-local-password certificate without placing a PFX
secret in shell history:

  powershell -ExecutionPolicy Bypass -File .\finalize-viogpu3d-test-package.ps1 `
    -PackageDir C:\path\bridgevm-viogpu3d-arm64-package `
    -PreFinalizationManifest C:\path\bridgevm-viogpu3d-arm64-package-pre-finalization.sha256

For a separately managed signing identity, invoke the audited finalizer:

  powershell -ExecutionPolicy Bypass -File .\finalize-viogpu3d-package.ps1 `
    -PackageDir C:\path\bridgevm-viogpu3d-arm64-package `
    -PreFinalizationManifest C:\path\bridgevm-viogpu3d-arm64-package-pre-finalization.sha256 `
    -CertificatePfx C:\path\BridgeVM-Test.pfx `
    -Profile arehnman-arm64-minimal

The finalizer discovers installed Windows Kits tools even when they are not in
the current PATH. It requires the PFX certificate to be trusted on the build
machine, validates the manifest and exact minimal-profile file set, and writes a separate
bridgevm-viogpu3d-arm64-package-finalized directory. It never mutates the
unsigned input. Copy only the finalized directory back to the Mac and run:

  scripts/check-hvf-windows-viogpu3d-package.sh \
    --manifest /tmp/viogpu3d-package-manifest.txt \
    --require-render-candidate \
    /path/to/bridgevm-viogpu3d-arm64-package-finalized

The checker auto-loads bridgevm-package-provenance.env from the package
directory, including the pinned source commits, build id, signing note,
protocol, and expected DEV_1050 HWID.

BridgeVM defaults to the Venus runtime for installed P3 boots. This VirGL
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
[[ ! -e "$OUT_DIR" ]] || fail "output path already exists: $OUT_DIR"

clone_or_fetch_source

if [[ -f "$SOURCE_DIR/viogpu/viogpu3d/viogpu3d_arm64.inx" ]]; then
  INX="$SOURCE_DIR/viogpu/viogpu3d/viogpu3d_arm64.inx"
else
  INX="$SOURCE_DIR/viogpu/viogpu3d/viogpu3d.inx"
fi
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
[[ "$protocol" == "virgl" || "$protocol" == "mixed" ]] ||
  fail "source does not expose the required VirGL/D3D10 path (detected protocol=$protocol)"
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
cp "$ROOT/scripts/finalize-hvf-windows-viogpu3d-package.ps1" \
  "$OUT_DIR/finalize-viogpu3d-package.ps1"
cp "$ROOT/scripts/finalize-hvf-windows-viogpu3d-test-package.ps1" \
  "$OUT_DIR/finalize-viogpu3d-test-package.ps1"
cp "$ROOT/scripts/win-assets/viogpu3d-arehnman-arm64-minimal.inf" \
  "$OUT_DIR/viogpu3d-arehnman-arm64-minimal.inf"
cp "$ROOT/scripts/win-assets/build-mesa-arm64.ps1" \
  "$OUT_DIR/build-mesa-arm64.ps1"
cp "$ROOT/scripts/win-assets/mesa-cross-arm64.ini" \
  "$OUT_DIR/mesa-cross-arm64.ini"
cp "$ROOT/scripts/win-assets/run-submit-trace-build.cmd" \
  "$OUT_DIR/run-submit-trace-build.cmd"
cp "$ROOT/scripts/patches/virtio-win-mesa-unbound-clear.patch" \
  "$OUT_DIR/virtio-win-mesa-unbound-clear.patch"
cp "$ROOT/scripts/patches/virtio-win-mesa-submit-trace.patch" \
  "$OUT_DIR/virtio-win-mesa-submit-trace.patch"

REPORT="$OUT_DIR/source-report.txt"
{
  printf 'BridgeVM viogpu3d external build kit\n'
  printf 'generated_utc=%s\n' "$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
  printf 'source_repo=%s\n' "$SOURCE_REPO"
  printf 'source_ref=%s\n' "$SOURCE_REF"
  printf 'source_dir=%s\n' "$SOURCE_DIR"
  printf 'source_head=%s\n' "$git_head"
  printf 'builder_default_driver_ref=4c27e477e6560cea724d848b98149f03cb1f2083\n'
  printf 'builder_default_mesa_repo=https://github.com/arehnman/virtio-win-mesa.git\n'
  printf 'builder_default_mesa_ref=cb531c440ff34a9c6334859dda0848132be49ec3\n'
  printf 'builder_host_arch=x64\n'
  printf 'builder_target_arch=arm64\n'
  printf 'builder_mesa_toolchain=clang-cl-cross\n'
  printf 'driver_solution=%s\n' "$SLN"
  printf 'driver_project=%s\n' "$VCXPROJ"
  printf 'inf_template=%s\n' "$INX"
  printf 'source_protocol=%s\n' "$protocol"
  printf 'protocol=virgl\n'
  printf 'package_profile=arehnman-arm64-minimal\n'
  printf 'hwids=%s\n' "$hwids"
  printf 'arm64_configuration_present=%s\n' "$arm64_config"
  printf 'mesa_prefix_required=%s\n' "$mesa_prefix_required"
  printf 'required_mesa_dlls=viogpu_d3d10.dll,viogpu_wgl.dll,opengl32.dll,libEGL.dll,libGLESv2.dll\n'
  printf 'staged_render_dlls=viogpu_d3d10_arm64.dll,viogpu_wgl_arm64.dll,opengl32_arm64.dll,libEGL_arm64.dll,libGLESv2_arm64.dll\n'
  printf 'windows_build_script=%s\n' "$OUT_DIR/build-viogpu3d-arm64.ps1"
  printf 'windows_submit_trace_runner=%s\n' "$OUT_DIR/run-submit-trace-build.cmd"
  printf 'windows_finalizer=%s\n' "$OUT_DIR/finalize-viogpu3d-package.ps1"
  printf 'windows_test_certificate_finalizer=%s\n' "$OUT_DIR/finalize-viogpu3d-test-package.ps1"
  printf 'pre_finalization_manifest_required=true\n'
  printf 'finalizer_mutates_unsigned_input=false\n'
  printf 'windows_finalization_order=InfVerif,PE-sign,Inf2Cat,CAT-sign,SignTool-verify\n'
  printf 'render_candidate_verified=false\n'
  printf 'finalization_required=true\n'
  printf 'readme=%s\n' "$OUT_DIR/README.txt"
  printf 'bridgevm_default_installed_host_protocol=venus\n'
  printf 'bridgevm_required_installed_host_protocol=virgl\n'
  printf 'boot_runtime_selector=--gpu-trace-protocol virgl\n'
  printf 'boot_blocker=windows WDK finalization and repository render-candidate check required\n'
} > "$REPORT"

cat "$REPORT"
