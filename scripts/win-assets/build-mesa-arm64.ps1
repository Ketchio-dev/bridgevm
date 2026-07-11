<#
.SYNOPSIS
  Cross-build the pinned VirGL/D3D10/OpenGL Mesa payload for Windows ARM64.

.DESCRIPTION
  Run from an x64 Visual Studio developer PowerShell. Native Meson tools remain
  x64 while clang-cl/lld-link/llvm-lib target arm64-pc-windows-msvc. Vulkan and
  GLES1 are deliberately disabled because BridgeVM's pinned minimal package
  carries only the five D3D10/OpenGL/EGL/GLES2 DLLs.
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$MesaSrc,
    [Parameter(Mandatory = $true)][string]$Prefix,
    [Parameter(Mandatory = $true)][string]$CrossFileBase
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Require-Command {
    param([Parameter(Mandatory = $true)][string]$Name)
    if (-not (Get-Command $Name -ErrorAction SilentlyContinue)) {
        throw "Required ARM64 cross-build command is not in PATH: $Name"
    }
}

function Require-EnvironmentValue {
    param(
        [Parameter(Mandatory = $true)][string]$Name,
        [AllowEmptyString()][string]$Value
    )
    if ([string]::IsNullOrWhiteSpace($Value)) {
        throw "Environment variable $Name is not set; run from an x64 Visual Studio developer PowerShell"
    }
    return $Value
}

foreach ($tool in @("cl.exe", "clang-cl.exe", "lld-link.exe", "llvm-lib.exe", "llvm-rc.exe", "llvm-strip.exe", "meson.exe", "ninja.exe")) {
    Require-Command $tool
}

$MesaSrc = [System.IO.Path]::GetFullPath($MesaSrc)
$Prefix = [System.IO.Path]::GetFullPath($Prefix)
$CrossFileBase = [System.IO.Path]::GetFullPath($CrossFileBase)
if (-not (Test-Path -LiteralPath (Join-Path $MesaSrc "meson.build") -PathType Leaf)) {
    throw "Mesa source tree is missing meson.build: $MesaSrc"
}
if (-not (Test-Path -LiteralPath $CrossFileBase -PathType Leaf)) {
    throw "BridgeVM ARM64 Meson cross file is missing: $CrossFileBase"
}
if (Test-Path -LiteralPath $Prefix) {
    if (@(Get-ChildItem -LiteralPath $Prefix -Force).Count -ne 0) {
        throw "Mesa ARM64 prefix must be empty to prevent stale DLL reuse: $Prefix"
    }
} else {
    New-Item -ItemType Directory -Path $Prefix | Out-Null
}

$vcTools = Require-EnvironmentValue "VCToolsInstallDir" $env:VCToolsInstallDir
$sdkDir = Require-EnvironmentValue "WindowsSdkDir" $env:WindowsSdkDir
$sdkVersion = $env:WindowsSDKVersion
if ([string]::IsNullOrWhiteSpace($sdkVersion)) {
    $sdkVersion = $env:WindowsSDKLibVersion
}
$sdkVersion = (Require-EnvironmentValue "WindowsSDKVersion" $sdkVersion).TrimEnd("\")

$vcArm64Lib = Join-Path $vcTools "lib\arm64"
$ucrtArm64 = Join-Path $sdkDir ("Lib\{0}\ucrt\arm64" -f $sdkVersion)
$umArm64 = Join-Path $sdkDir ("Lib\{0}\um\arm64" -f $sdkVersion)
foreach ($path in @($vcArm64Lib, $ucrtArm64, $umArm64)) {
    if (-not (Test-Path -LiteralPath $path -PathType Container)) {
        throw "ARM64 import-library path is missing; install ARM64 VS/SDK tools: $path"
    }
}

# The pinned Mesa revision exposes mesa::float16_t globally, which collides with
# clang-cl ARM64's arm_neon.h typedef. Apply the exact idempotent source rewrite
# used by the preserved successful CI build.
$glslDir = Join-Path $MesaSrc "src\compiler\glsl"
if (-not (Test-Path -LiteralPath $glslDir -PathType Container)) {
    throw "Pinned Mesa GLSL source directory is missing: $glslDir"
}
$patchedFiles = 0
$utf8NoBom = New-Object System.Text.UTF8Encoding($false)
Get-ChildItem -LiteralPath $glslDir -Recurse -Include "*.h", "*.cpp" -File | ForEach-Object {
    $content = Get-Content -LiteralPath $_.FullName -Raw
    $original = $content
    $content = $content -replace '(?m)^\s*using\s+float16_t\s*=\s*mesa::float16_t\s*;\s*\r?\n', ''
    $content = [regex]::Replace($content, '(?<![\w:.>])float16_t\b', 'mesa::float16_t')
    if ($content -cne $original) {
        [System.IO.File]::WriteAllText($_.FullName, $content, $utf8NoBom)
        $patchedFiles += 1
    }
}
if ($patchedFiles -eq 0) {
    throw "Pinned Mesa float16_t patch made no changes; source contract may have drifted"
}

$linkArguments = @($vcArm64Lib, $ucrtArm64, $umArm64) | ForEach-Object {
    "'/libpath:$($_.Replace('\', '\\'))'"
}
$linkArgumentText = $linkArguments -join ", "
$compilerArguments = @(
    "-D_interlockedincrement64=_InterlockedIncrement64",
    "-D_interlockeddecrement64=_InterlockedDecrement64",
    "-D_interlockedexchange64=_InterlockedExchange64",
    "-D_interlockedexchangeadd64=_InterlockedExchangeAdd64",
    "-D_interlockedadd(a,b)=(_InterlockedExchangeAdd((a),(b))+(b))",
    "-D_interlockedadd64(a,b)=(_InterlockedExchangeAdd64((a),(b))+(b))"
)
$compilerArgumentText = ($compilerArguments | ForEach-Object { "'$_'" }) -join ", "
$augmentation = @"

[built-in options]
c_args = [$compilerArgumentText]
cpp_args = [$compilerArgumentText]
c_link_args = [$linkArgumentText]
cpp_link_args = [$linkArgumentText]
"@
$tempRoot = $env:RUNNER_TEMP
if ([string]::IsNullOrWhiteSpace($tempRoot)) {
    $tempRoot = [System.IO.Path]::GetTempPath()
}
$generatedCrossFile = Join-Path $tempRoot ("bridgevm-mesa-cross-arm64-" + [Guid]::NewGuid().ToString("N") + ".ini")
$buildDir = Join-Path $MesaSrc "build-arm64"
if (Test-Path -LiteralPath $buildDir) {
    throw "Mesa ARM64 build directory already exists; use a clean source checkout: $buildDir"
}

try {
    [System.IO.File]::WriteAllText(
        $generatedCrossFile,
        (Get-Content -LiteralPath $CrossFileBase -Raw) + $augmentation
    )
    Push-Location $MesaSrc
    try {
        & meson.exe setup $buildDir `
            --cross-file $generatedCrossFile `
            --prefix $Prefix `
            -Dgallium-drivers=virgl `
            -Dgallium-d3d10umd=true `
            -Dgallium-wgl-dll-name=viogpu_wgl `
            -Dgallium-d3d10-dll-name=viogpu_d3d10 `
            -Dvulkan-drivers= `
            -Dzlib=disabled `
            -Dc_std=c11 `
            -Db_vscrt=mt `
            -Dllvm=disabled `
            -Ddraw-use-llvm=false `
            -Dshared-glapi=enabled `
            -Dgles1=disabled `
            -Dgles2=enabled
        if ($LASTEXITCODE -ne 0) {
            throw "Mesa ARM64 configuration failed with exit code $LASTEXITCODE"
        }
        & ninja.exe -C $buildDir install
        if ($LASTEXITCODE -ne 0) {
            throw "Mesa ARM64 build/install failed with exit code $LASTEXITCODE"
        }
    } finally {
        Pop-Location
    }
} finally {
    Remove-Item -LiteralPath $generatedCrossFile -Force -ErrorAction SilentlyContinue
}

$requiredDllNames = @(
    "libEGL.dll",
    "libGLESv2.dll",
    "opengl32.dll",
    "viogpu_d3d10.dll",
    "viogpu_wgl.dll"
)
foreach ($name in $requiredDllNames) {
    $path = Join-Path (Join-Path $Prefix "bin") $name
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
        throw "Mesa ARM64 build is missing required minimal-profile payload: $path"
    }
}

Write-Host "BridgeVM Mesa ARM64 minimal payload built at $Prefix"
