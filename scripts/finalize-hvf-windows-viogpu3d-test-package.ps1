[CmdletBinding()]
param(
    [string]$PackageDir = (Join-Path $PSScriptRoot "package"),
    [string]$FinalizedDir = "",
    [string]$PreFinalizationManifest = (Join-Path $PSScriptRoot "pre-finalization-sha256.txt"),
    [string]$Finalizer = (Join-Path $PSScriptRoot "finalize-viogpu3d-package.ps1"),
    [string]$Subject = "CN=BridgeVM viogpu3d Test",
    [switch]$KeepPrivateCertificate
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Invoke-CertUtil {
    param(
        [Parameter(Mandatory = $true)][string[]]$Arguments,
        [Parameter(Mandatory = $true)][string]$Label,
        [switch]$IgnoreFailure
    )

    # Import-Certificate can return E_ACCESSDENIED for LocalMachine\TrustedPublisher
    # on Windows 11 ARM64 even when the caller is elevated and LocalMachine\My is
    # writable. CertUtil uses the native machine-store path that Windows driver
    # provisioning itself relies on. Capture its output so a failure is actionable.
    $previousErrorActionPreference = $ErrorActionPreference
    try {
        $ErrorActionPreference = "Continue"
        $output = (& certutil.exe @Arguments 2>&1 | Out-String)
        $exitCode = $LASTEXITCODE
    } finally {
        $ErrorActionPreference = $previousErrorActionPreference
    }
    if ($exitCode -ne 0 -and -not $IgnoreFailure) {
        throw "$Label failed with exit code $exitCode`n$output"
    }
}

$principal = [Security.Principal.WindowsPrincipal]::new(
    [Security.Principal.WindowsIdentity]::GetCurrent()
)
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    throw "Test-package finalization must run from an elevated PowerShell"
}

$PackageDir = [System.IO.Path]::GetFullPath($PackageDir)
$PreFinalizationManifest = [System.IO.Path]::GetFullPath($PreFinalizationManifest)
$Finalizer = [System.IO.Path]::GetFullPath($Finalizer)
if ($FinalizedDir -eq "") {
    $FinalizedDir = "$PackageDir-finalized"
}
$FinalizedDir = [System.IO.Path]::GetFullPath($FinalizedDir)

foreach ($required in @($PackageDir, $PreFinalizationManifest, $Finalizer)) {
    if (-not (Test-Path -LiteralPath $required)) {
        throw "Required finalization input does not exist: $required"
    }
}

$certificate = $null
$temporaryPfx = Join-Path ([System.IO.Path]::GetTempPath()) ("bridgevm-viogpu3d-" + [Guid]::NewGuid().ToString("N") + ".pfx")
$temporaryCer = Join-Path ([System.IO.Path]::GetTempPath()) ("bridgevm-viogpu3d-" + [Guid]::NewGuid().ToString("N") + ".cer")
$passwordBytes = New-Object byte[] 32
$rng = [System.Security.Cryptography.RandomNumberGenerator]::Create()
$rng.GetBytes($passwordBytes)
$rng.Dispose()
$plainPassword = [System.Convert]::ToBase64String($passwordBytes)
$securePassword = ConvertTo-SecureString -String $plainPassword -AsPlainText -Force
$previousPassword = $env:VIOGPU3D_CERTIFICATE_PASSWORD
$finalizationSucceeded = $false

try {
    $certificate = New-SelfSignedCertificate `
        -Type CodeSigningCert `
        -Subject $Subject `
        -CertStoreLocation "Cert:\LocalMachine\My" `
        -KeyExportPolicy Exportable `
        -KeyLength 2048 `
        -HashAlgorithm SHA256 `
        -NotAfter (Get-Date).AddYears(5)
    if ($null -eq $certificate -or -not $certificate.HasPrivateKey) {
        throw "Failed to create an exportable code-signing certificate"
    }

    Export-Certificate -Cert $certificate -FilePath $temporaryCer -Force | Out-Null
    Invoke-CertUtil -Arguments @("-f", "-addstore", "Root", $temporaryCer) -Label "CertUtil Root import"
    Invoke-CertUtil -Arguments @("-f", "-addstore", "TrustedPublisher", $temporaryCer) -Label "CertUtil TrustedPublisher import"
    Export-PfxCertificate -Cert $certificate -FilePath $temporaryPfx -Password $securePassword -Force | Out-Null

    # The audited finalizer reads this process-local variable to open the PFX,
    # then signs by LocalMachine\My thumbprint and suppresses the variable while
    # SDK/WDK children run. The random password therefore appears in neither a
    # child process command line/environment nor the BVAGENT control log.
    $env:VIOGPU3D_CERTIFICATE_PASSWORD = $plainPassword
    & $Finalizer `
        -PackageDir $PackageDir `
        -FinalizedDir $FinalizedDir `
        -PreFinalizationManifest $PreFinalizationManifest `
        -CertificatePfx $temporaryPfx `
        -TestSigning

    $report = Join-Path $FinalizedDir "bridgevm-finalization-report.txt"
    if (-not (Test-Path -LiteralPath $report -PathType Leaf) -or
        -not (Select-String -LiteralPath $report -SimpleMatch "finalization_complete=true" -Quiet) -or
        -not (Select-String -LiteralPath $report -SimpleMatch "test_signing_required=true" -Quiet)) {
        throw "Finalizer returned without a complete finalization report: $report"
    }
    $finalizationSucceeded = $true
    Write-Host "BridgeVM viogpu3d disposable test certificate: $($certificate.Thumbprint)"
    Write-Host "Private PFX will be deleted; public trust remains for live package installation"
    Write-Host "Windows TESTSIGNING must be enabled before this package can bind"
} finally {
    if ($null -eq $previousPassword) {
        Remove-Item Env:\VIOGPU3D_CERTIFICATE_PASSWORD -ErrorAction SilentlyContinue
    } else {
        $env:VIOGPU3D_CERTIFICATE_PASSWORD = $previousPassword
    }
    Remove-Item -LiteralPath $temporaryPfx -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath $temporaryCer -Force -ErrorAction SilentlyContinue
    if ($null -ne $certificate -and -not $finalizationSucceeded) {
        # A failed package cannot use this trust anchor. Avoid leaving public
        # test trust behind unless finalization produced the audited report.
        Invoke-CertUtil -Arguments @("-delstore", "Root", $certificate.Thumbprint) -Label "CertUtil Root cleanup" -IgnoreFailure
        Invoke-CertUtil -Arguments @("-delstore", "TrustedPublisher", $certificate.Thumbprint) -Label "CertUtil TrustedPublisher cleanup" -IgnoreFailure
    }
    if ($null -ne $certificate -and -not $KeepPrivateCertificate) {
        Remove-Item -LiteralPath ("Cert:\LocalMachine\My\" + $certificate.Thumbprint) -Force -ErrorAction SilentlyContinue
    }
    if ($null -ne $certificate) {
        $certificate.Dispose()
    }
    $plainPassword = $null
    $securePassword = $null
    [Array]::Clear($passwordBytes, 0, $passwordBytes.Length)
}
