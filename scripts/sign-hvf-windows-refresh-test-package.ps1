# Development signing only, on an elevated disposable Windows builder.
# Never use this certificate for a product release.
param(
    [Parameter(Mandatory = $true)][string]$InputDirectory,
    [Parameter(Mandatory = $true)][string]$OutputDirectory,
    [Parameter(Mandatory = $true)][string]$ToolRoot
)
$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest
$InputDirectory = (Resolve-Path -LiteralPath $InputDirectory).Path
$OutputDirectory = [System.IO.Path]::GetFullPath($OutputDirectory)
if (Test-Path -LiteralPath $OutputDirectory) { throw "Choose a new output directory" }
function Find-Tool([string]$Name, [string]$Architecture) {
    $tool = Get-ChildItem -LiteralPath $ToolRoot -Filter $Name -File -Recurse |
        Where-Object { $_.Directory.Name -eq $Architecture } | Select-Object -First 1
    if ($null -eq $tool) { throw "Missing SDK/WDK tool: $Name ($Architecture)" }
    return $tool.FullName
}
$signtool = Find-Tool "signtool.exe" "x64"
$inf2cat = Find-Tool "Inf2Cat.exe" "x86"
$help = (& $inf2cat /? 2>&1 | Out-String)
$os = @("10_GE_ARM64", "10_NI_ARM64", "10_CO_ARM64") |
    Where-Object { $help -match "\b$_\b" } | Select-Object -First 1
if ($null -eq $os) { throw "Inf2Cat has no supported Windows 11 ARM64 token" }
New-Item -ItemType Directory -Path $OutputDirectory | Out-Null
foreach ($name in @("viogpudo.sys", "viogpudo.inf", "LICENSE")) {
    Copy-Item -LiteralPath (Join-Path $InputDirectory $name) -Destination $OutputDirectory
}
$certificate = New-SelfSignedCertificate -Type CodeSigningCert `
    -Subject "CN=BridgeVM Refresh Development Test" -KeyAlgorithm RSA -KeyLength 3072 `
    -HashAlgorithm SHA256 -KeyExportPolicy NonExportable -CertStoreLocation Cert:\CurrentUser\My `
    -NotBefore (Get-Date).AddMinutes(-15) -NotAfter (Get-Date).AddDays(30)
$rootPath = "Cert:\LocalMachine\Root\$($certificate.Thumbprint)"
$myPath = "Cert:\CurrentUser\My\$($certificate.Thumbprint)"
try {
    $sys = Join-Path $OutputDirectory "viogpudo.sys"
    $cat = Join-Path $OutputDirectory "viogpudo.cat"
    $cer = Join-Path $OutputDirectory "BridgeVM-Refresh-Test.cer"
    & $signtool sign /fd SHA256 /sha1 $certificate.Thumbprint /s My $sys
    if ($LASTEXITCODE -ne 0) { throw "Test PE signing failed" }
    & $inf2cat "/driver:$OutputDirectory" "/os:$os" /uselocaltime
    if ($LASTEXITCODE -ne 0) { throw "Test catalog generation failed" }
    & $signtool sign /fd SHA256 /sha1 $certificate.Thumbprint /s My $cat
    if ($LASTEXITCODE -ne 0) { throw "Test catalog signing failed" }
    Export-Certificate -Cert $certificate -FilePath $cer | Out-Null
    # A user's Root store can display an interactive trust prompt. This
    # disposable elevated builder uses machine trust only for this verification,
    # then removes it below. The non-exportable private key stays in My.
    Write-Host "Checking test signatures with temporary disposable-builder trust"
    Import-Certificate -FilePath $cer -CertStoreLocation Cert:\LocalMachine\Root | Out-Null
    foreach ($file in @($sys, $cat)) {
        & $signtool verify /pa $file
        if ($LASTEXITCODE -ne 0) { throw "Test signature verification failed: $file" }
    }
    @"
TEST SIGNING ONLY - NOT A PRODUCTION WINDOWS SIGNATURE
Requires an explicitly authorized disposable VM with Secure Boot disabled and
Windows TESTSIGNING enabled. Import only the public test certificate there.
Do not install this package in the working VM or change the Mac's trust settings.
Certificate SHA1: $($certificate.Thumbprint)
Expires: $($certificate.NotAfter.ToUniversalTime().ToString('o'))
"@ | Set-Content (Join-Path $OutputDirectory "TEST-ONLY.txt") -Encoding UTF8
    $sums = Get-ChildItem -LiteralPath $OutputDirectory -File | Sort-Object Name | ForEach-Object {
        $digest = (Get-FileHash -Algorithm SHA256 -LiteralPath $_.FullName).Hash.ToLowerInvariant()
        "$digest  $($_.Name)"
    }
    [System.IO.File]::WriteAllText((Join-Path $OutputDirectory "SHA256SUMS"),
        ($sums -join "`n") + "`n", [System.Text.Encoding]::ASCII)
} finally {
    if (Test-Path $rootPath) { Remove-Item $rootPath -Force }
    if (Test-Path $myPath) { Remove-Item $myPath -DeleteKey -Force }
}
