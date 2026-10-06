param([string]$Python = 'python')
# The copied real guest script sees only mocked providers and owned tiny files.
$ErrorActionPreference = 'Stop'
$source = Join-Path $PSScriptRoot '../../scripts/win-assets/bv-d11-fixture-ready.ps1'
$root = Join-Path ([IO.Path]::GetTempPath()) ([Guid]::NewGuid().ToString())
[IO.Directory]::CreateDirectory($root) | Out-Null
$global:BridgeVMD11Case = ''
$global:BridgeVMD11PolicyCalls = 0
$global:BridgeVMD11Created = 0
$global:BridgeVMD11RequestedFields = @()
$identity = [pscustomobject]@{ Name = 'SYNTHETIC\BVT17111111111111'; User = [pscustomobject]@{ Value = 'synthetic-owned-sid' } }
function Get-D11SyntheticIdentity { return $identity }
function Get-LocalUser {
    param([string]$Name)
    if ($Name -cne ('BVT17' + ('1' * 12))) { throw 'unexpected fixture account' }
    $sid = $identity.User.Value
    if ($global:BridgeVMD11Case -eq 'wrong-account') { $sid = 'synthetic-other-sid' }
    $expires = $null
    if ($global:BridgeVMD11Case -eq 'policy-refusal') { $expires = [DateTime]::UtcNow }
    return [pscustomobject]@{ Enabled = $true; SID = [pscustomobject]@{ Value = $sid }; PasswordExpires = $expires }
}
function Set-LocalUser {
    param([string]$Name, [bool]$PasswordNeverExpires)
    if ($Name -cne ('BVT17' + ('1' * 12)) -or -not $PasswordNeverExpires) { throw 'wrong policy target' }
    $global:BridgeVMD11PolicyCalls++
}
function Get-ScheduledTask {
    param([string]$TaskName)
    if ($TaskName -cne 'BridgeVM Guest Agent') { throw 'unexpected task' }
    $owner = $identity.Name
    if ($global:BridgeVMD11Case -eq 'wrong-task') { $owner = 'synthetic-other-owner' }
    return [pscustomobject]@{ Principal = [pscustomobject]@{ UserId = $owner; LogonType = 'Interactive'; RunLevel = 'Highest' }; State = 'Running' }
}
function Get-Process {
    param([int]$Id, [string]$Name, [string]$ErrorAction)
    if ($Name -eq 'explorer' -and $global:BridgeVMD11Case -eq 'no-desktop') { return }
    return [pscustomobject]@{ SessionId = 2 }
}
function Get-ItemPropertyValue {
    param([string]$LiteralPath, [string]$Name)
    $global:BridgeVMD11RequestedFields += $Name
    switch ($Name) {
        AutoLogonCount { if ($global:BridgeVMD11Case -eq 'no-logons') { return [int]0 }; return [int]3 }
        AutoAdminLogon { return '1' }
        DefaultUserName { return 'BVT17' + ('1' * 12) }
        default { throw 'secret or unexpected registry field requested' }
    }
}
function Invoke-CimMethod {
    param([string]$ClassName, [string]$MethodName, [hashtable]$Arguments)
    if ($ClassName -cne 'Win32_Process' -or $MethodName -cne 'Create' -or $Arguments.CommandLine -notmatch ' -File .* -Action Prepare ') {
        throw 'workload did not use the file/CIM channel'
    }
    $global:BridgeVMD11Created++
    return [pscustomobject]@{ ReturnValue = 0; ProcessId = 17 }
}
try {
    foreach ($case in @('success', 'wrong-account', 'wrong-task', 'no-desktop', 'no-logons', 'policy-refusal', 'launch')) {
        $global:BridgeVMD11Case = $case; $global:BridgeVMD11PolicyCalls = 0; $global:BridgeVMD11Created = 0; $global:BridgeVMD11RequestedFields = @()
        $dir = Join-Path $root $case; [IO.Directory]::CreateDirectory($dir) | Out-Null
        $copy = Join-Path $dir 'bv-d11-fixture-ready.ps1'
        $body = [IO.File]::ReadAllText($source)
        $provider = '[Security.Principal.WindowsIdentity]::GetCurrent()'
        if (($body.Split(@($provider), [StringSplitOptions]::None)).Count -ne 2) { throw 'identity provider seam differs' }
        [IO.File]::WriteAllText($copy, $body.Replace($provider, '(Get-D11SyntheticIdentity)'), (New-Object Text.UTF8Encoding($false)))
        $hash = (Get-FileHash -LiteralPath $copy -Algorithm SHA256).Hash.ToLowerInvariant()
        $nonce = '1' * 64
        $action = 'Prepare'; if ($case -eq 'launch') { $action = 'Launch' }
        $failed = $false; $failureNote = ''
        try { $reply = & $copy -Action $action -Nonce $nonce -ExpectedSha256 $hash } catch { $failed = $true; $failureNote = $_.Exception.Message }
        $done = Join-Path $dir "result-$nonce.done"
        if ($case -eq 'success') {
            if ($failed -or -not (Test-Path -LiteralPath $done) -or $global:BridgeVMD11PolicyCalls -ne 1) { throw ('success transition failed: ' + $failureNote) }
            $report = Get-Content -LiteralPath (Join-Path $dir "result-$nonce.json") -Raw | ConvertFrom-Json
            if ($report.account_bound -ne $true -or $report.password_never_expires -ne $true -or $report.autologon_remaining -ne 3) { throw 'invalid readiness facts' }
        } elseif ($case -eq 'launch') {
            if ($failed -or $global:BridgeVMD11Created -ne 1 -or $global:BridgeVMD11PolicyCalls -ne 0 -or $reply -cne "D11-FIXTURE-LAUNCHED-$nonce") { throw 'launch contract failed' }
        } else {
            if (-not $failed -or (Test-Path -LiteralPath $done)) { throw 'refusal produced a completed result' }
            $expected = 0; if ($case -eq 'policy-refusal') { $expected = 1 }
            if ($global:BridgeVMD11PolicyCalls -ne $expected) { throw 'policy mutation before account/readiness binding' }
        }
        if ($global:BridgeVMD11RequestedFields -contains 'DefaultPassword') { throw 'password value accessed' }
    }
    Write-Output 'PASS: 7 D11 mocked Windows readiness/refusal/launch cases'
} finally { Remove-Item -LiteralPath $root -Recurse -Force }
