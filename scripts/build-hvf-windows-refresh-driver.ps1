# Build an unsigned ARM64 candidate. This script never installs a driver or changes boot security.
param(
    [string]$SourceDirectory = (Join-Path $PSScriptRoot "source"),
    [string]$OutputDirectory = (Join-Path $PSScriptRoot "unsigned-arm64"),
    [switch]$UseNuGet
)
$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest
$SourceDirectory = (Resolve-Path -LiteralPath $SourceDirectory).Path
$OutputDirectory = [System.IO.Path]::GetFullPath($OutputDirectory)
if (-not (Test-Path (Join-Path $SourceDirectory "viogpu\viogpudo\viogpudo.vcxproj"))) {
    throw "Use the source exported by prepare-hvf-windows-refresh-build-kit.sh"
}

# Existing VS2022 + WDK installations need no package restore. The optional
# NuGet route installs Microsoft's SDK/WDK packages and requires their licence
# acceptance; the operator must authorize that separately before using it.
if ($UseNuGet) {
    $version = "10.0.26100.6584"
    @"
<?xml version="1.0" encoding="utf-8"?>
<packages>
  <package id="Microsoft.Windows.WDK.arm64" version="$version" targetFramework="native" />
  <package id="Microsoft.Windows.SDK.CPP.arm64" version="$version" targetFramework="native" />
  <package id="Microsoft.Windows.SDK.CPP" version="$version" targetFramework="native" />
</packages>
"@ | Set-Content -LiteralPath (Join-Path $SourceDirectory "packages.config") -Encoding UTF8
    @'
<Project>
  <PropertyGroup>
    <DriverNuGetRoot>$(MSBuildThisFileDirectory)packages\</DriverNuGetRoot>
    <DriverKitNuGetVersion>10.0.26100.6584</DriverKitNuGetVersion>
  </PropertyGroup>
  <Import Project="$(DriverNuGetRoot)Microsoft.Windows.WDK.arm64.$(DriverKitNuGetVersion)\build\native\Microsoft.Windows.WDK.arm64.props" />
  <Import Project="$(DriverNuGetRoot)Microsoft.Windows.SDK.CPP.arm64.$(DriverKitNuGetVersion)\build\native\Microsoft.Windows.SDK.cpp.arm64.props" />
  <Import Project="$(DriverNuGetRoot)Microsoft.Windows.SDK.CPP.$(DriverKitNuGetVersion)\build\native\Microsoft.Windows.SDK.cpp.props" />
</Project>
'@ | Set-Content -LiteralPath (Join-Path $SourceDirectory "Directory.Build.props") -Encoding UTF8
    & nuget.exe restore (Join-Path $SourceDirectory "packages.config") `
        -PackagesDirectory (Join-Path $SourceDirectory "packages") -NonInteractive
    if ($LASTEXITCODE -ne 0) { throw "SDK/WDK restore failed" }
}

$msbuild = Get-Command msbuild.exe -ErrorAction SilentlyContinue
if ($null -eq $msbuild) {
    $vswhere = Join-Path ${env:ProgramFiles(x86)} "Microsoft Visual Studio\Installer\vswhere.exe"
    $vsRoot = & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.ARM64 -property installationPath
    if ($LASTEXITCODE -ne 0 -or [string]::IsNullOrWhiteSpace($vsRoot)) { throw "VS2022 ARM64 C++ tools are required" }
    $msbuildPath = Join-Path $vsRoot "MSBuild\Current\Bin\MSBuild.exe"
} else {
    $msbuildPath = $msbuild.Source
}
# The ARM64 NuGet kit provides x64/ARM64 verifier DLLs. The VS Bin entry
# found on PATH can be 32-bit; use the x64 MSBuild host for its native tasks.
$amd64Msbuild = Join-Path (Split-Path $msbuildPath -Parent) "amd64\MSBuild.exe"
if (Test-Path -LiteralPath $amd64Msbuild) { $msbuildPath = $amd64Msbuild }
Push-Location $SourceDirectory
$previousLib = $env:LIB
$previousPath = $env:PATH
try {
    if ($UseNuGet) {
        # Upstream's INF stamping step invokes stampinf.exe by name. NuGet
        # supplies the host tools without modifying the process environment.
        $kitBin = Join-Path $SourceDirectory "packages\Microsoft.Windows.WDK.arm64.10.0.26100.6584\c\bin\10.0.26100.0"
        $env:PATH = "$(Join-Path $kitBin 'x64');$(Join-Path $kitBin 'x86');$previousPath"
    }
    # Some WDK toolsets normalize OutDir to an absolute path; the upstream
    # relative LIBPATH then contains two roots. Supply the real library path.
    $env:LIB = "$(Join-Path $SourceDirectory 'VirtIO\objfre_win11_arm64\arm64');$previousLib"
    foreach ($project in @("VirtIO\VirtioLib.vcxproj", "viogpu\viogpudo\viogpudo.vcxproj")) {
        & $msbuildPath $project /m /t:Build "/p:Configuration=Win11 Release" /p:Platform=ARM64 `
            /p:PreferredToolArchitecture=x64 /p:SignMode=Off /p:Feature_PackOne=false /p:_BUILD_MINOR_VERSION_=58013
        if ($LASTEXITCODE -ne 0) { throw "ARM64 build failed: $project" }
    }
    $built = Join-Path $SourceDirectory "viogpu\viogpudo\objfre_win11_arm64\arm64"
    $sys = Join-Path $built "viogpudo.sys"
    $inf = Join-Path $built "viogpudo.inf"
    if (-not (Test-Path $sys) -or -not (Test-Path $inf)) { throw "Expected SYS/INF outputs were not generated" }
    $pe = [System.IO.File]::ReadAllBytes($sys)
    $peOffset = [BitConverter]::ToInt32($pe, 0x3c)
    if ([BitConverter]::ToUInt16($pe, $peOffset + 4) -ne 0xaa64) { throw "Driver is not ARM64" }
    New-Item -ItemType Directory -Force -Path $OutputDirectory | Out-Null
    Copy-Item -LiteralPath $sys, $inf -Destination $OutputDirectory
    Copy-Item -LiteralPath (Join-Path $SourceDirectory "viogpu\viogpudo\LICENSE") -Destination $OutputDirectory
    Get-FileHash -Algorithm SHA256 -LiteralPath $sys, $inf | Format-Table -AutoSize
    Write-Host "Unsigned ARM64 candidate: $OutputDirectory"
    Write-Host "Microsoft production signing and disposable-VM validation remain required."
} finally {
    $env:LIB = $previousLib
    $env:PATH = $previousPath
    Pop-Location
}
