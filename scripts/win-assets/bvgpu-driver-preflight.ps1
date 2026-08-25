[CmdletBinding()]
param(
    [string]$PackageDirectory = "C:\BridgeVM\viogpu3d",
    [string]$OutPath = "C:\BridgeVM\viogpu3d-preflight.log",
    [switch]$SelfTest
)
Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Resolve-BvGpuPreflight {
    param([string]$SigningMode, [bool]$TestSigningRequired, [string]$SecureBoot,
        [string]$TestSigning, [string]$Certificate, [string]$PnpStatus,
        [int]$ProblemCode)
    $blocker = "none"
    if ($SigningMode -notin @("test", "kernel-policy")) { $blocker = "signing-mode-unverifiable" }
    elseif ($SigningMode -eq "kernel-policy") { $blocker = "kernel-policy-provenance-unverifiable" }
    elseif ($SecureBoot -eq "unknown") { $blocker = "secure-boot-state-unavailable" }
    elseif ($TestSigningRequired -and $SecureBoot -eq "enabled") { $blocker = "test-signing-blocked-by-secure-boot" }
    elseif ($TestSigning -eq "unknown") { $blocker = "test-signing-state-unavailable" }
    elseif ($TestSigningRequired -and $Certificate -ne "valid") { $blocker = "test-certificate-unverifiable" }
    [pscustomobject]@{ Result = $(if ($blocker -eq "none") { "ready" } else { "blocked" })
        Blocker = $blocker; SigningMode = $SigningMode; TestSigningRequired = $TestSigningRequired
        SecureBoot = $SecureBoot; TestSigning = $TestSigning; Certificate = $Certificate
        PnpStatus = $PnpStatus; ProblemCode = $ProblemCode }
}

function Assert-Case {
    param([string]$Expected, [hashtable]$State)
    $result = Resolve-BvGpuPreflight @State
    if ($result.Blocker -ne $Expected) { throw "expected $Expected, got $($result.Blocker)" }
}

if ($SelfTest) {
    $base = @{ SigningMode="test"; TestSigningRequired=$true; SecureBoot="disabled"
        TestSigning="disabled"; Certificate="valid"; PnpStatus="Error"; ProblemCode=52 }
    Assert-Case "none" $base
    $case = $base.Clone(); $case.SecureBoot="enabled"; Assert-Case "test-signing-blocked-by-secure-boot" $case
    $case = $base.Clone(); $case.SecureBoot="enabled"; $case.TestSigning="enabled"; Assert-Case "test-signing-blocked-by-secure-boot" $case
    $case = $base.Clone(); $case.Certificate="missing"
    Assert-Case "test-certificate-unverifiable" $case
    $case = $base.Clone(); $case.SecureBoot="unknown"
    Assert-Case "secure-boot-state-unavailable" $case
    $case = $base.Clone(); $case.SigningMode="unknown"
    Assert-Case "signing-mode-unverifiable" $case
    $case = $base.Clone(); $case.SigningMode="kernel-policy"; $case.TestSigningRequired=$false
    Assert-Case "kernel-policy-provenance-unverifiable" $case
    $case = $base.Clone(); $case.TestSigning="unknown"
    Assert-Case "test-signing-state-unavailable" $case
    Write-Output "PASS: bvgpu preflight self-test (8 cases)"; exit 0
}

$reportPath = Join-Path $PackageDirectory "bridgevm-finalization-report.txt"
if (-not (Test-Path -LiteralPath $reportPath -PathType Leaf)) {
    $signingMode = "unknown"; $testSigningRequired = $true
} else {
    $report = [IO.File]::ReadAllText($reportPath)
    $signingMode = if ($report -match '(?m)^signing_mode=(test|kernel-policy)$') { $Matches[1] } else { "unknown" }
    $testSigningRequired = [bool]($report -match '(?m)^test_signing_required=true$')
}
try { $secureBoot = if (Confirm-SecureBootUEFI -ErrorAction Stop) { "enabled" } else { "disabled" } }
catch { $secureBoot = "unknown" }
$bcd = (& bcdedit.exe /enum '{current}' 2>&1 | Out-String)
$testSigning = if ($LASTEXITCODE -ne 0) { "unknown" } elseif ($bcd -match '(?im)^testsigning\s+Yes\s*$') { "enabled" } else { "disabled" }
$cer = Join-Path $PackageDirectory "BridgeVM-viogpu3d-Test.cer"
try { $null = [Security.Cryptography.X509Certificates.X509Certificate2]::new($cer); $certificate = "valid" }
catch { $certificate = if (Test-Path -LiteralPath $cer -PathType Leaf) { "invalid" } else { "missing" } }
try {
    $dev = @(Get-PnpDevice -PresentOnly -ErrorAction Stop | Where-Object InstanceId -like 'PCI\VEN_1AF4&DEV_10F7*' | Select-Object -First 1)
    $pnpStatus = if ($dev.Count) { [string]$dev[0].Status } else { "absent" }
    $problem = if ($dev.Count) { Get-PnpDeviceProperty -InstanceId $dev[0].InstanceId -KeyName DEVPKEY_Device_ProblemCode -ErrorAction Stop } else { $null }
    $problemCode = if ($null -ne $problem) { [int]$problem.Data } else { -1 }
} catch { $pnpStatus = "unknown"; $problemCode = -1 }
$result = Resolve-BvGpuPreflight -SigningMode $signingMode -TestSigningRequired $testSigningRequired `
    -SecureBoot $secureBoot -TestSigning $testSigning -Certificate $certificate `
    -PnpStatus $pnpStatus -ProblemCode $problemCode
$line = "BVGPU_PREFLIGHT result=$($result.Result) blocker=$($result.Blocker) secure_boot=$secureBoot testsigning=$testSigning signing_mode=$signingMode test_signing_required=$($testSigningRequired.ToString().ToLowerInvariant()) certificate=$certificate pnp_status=$pnpStatus problem_code=$problemCode"
[IO.File]::WriteAllText($OutPath, $line + [Environment]::NewLine, [Text.Encoding]::ASCII)
Write-Output $line
if ($result.Result -ne "ready") { exit 2 }
