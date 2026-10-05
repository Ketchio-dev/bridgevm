[CmdletBinding()]
param([string]$Action, [string]$Nonce, [string]$ExpectedSha256)
# Explicitly enabled development fixture provisioning, never product first boot.
$ErrorActionPreference = 'Stop'
if ($Nonce -cnotmatch '^[0-9a-f]{64}\z' -or $ExpectedSha256 -cnotmatch '^[0-9a-f]{64}\z') { throw 'invalid fixture binding' }
$ActualSha256 = (Get-FileHash -LiteralPath $PSCommandPath -Algorithm SHA256).Hash.ToLowerInvariant()
if ($ActualSha256 -cne $ExpectedSha256) { throw 'fixture script differs' }
if ($Action -ceq 'Launch') {
    $command = "powershell.exe -NoProfile -ExecutionPolicy Bypass -File C:\BridgeVM\d11-fixture\bv-d11-fixture-ready.ps1 -Action Prepare -Nonce $Nonce -ExpectedSha256 $ExpectedSha256"
    $created = Invoke-CimMethod -ClassName Win32_Process -MethodName Create -Arguments @{ CommandLine = $command }
    if ($created.ReturnValue -ne 0 -or $created.ProcessId -le 0) { throw 'fixture workload launch failed' }
    Write-Output "D11-FIXTURE-LAUNCHED-$Nonce"
    return
}
if ($Action -cne 'Prepare') { throw 'invalid fixture action' }
$result = Join-Path $PSScriptRoot "result-$Nonce.json"
$done = Join-Path $PSScriptRoot "result-$Nonce.done"
$pending = Join-Path $PSScriptRoot "result-$Nonce.pending"
foreach ($name in @($result, $done, $pending)) {
    if ($null -ne (Get-Item -LiteralPath $name -Force -ErrorAction SilentlyContinue)) { throw 'fixture result already exists' }
}
$stream = [IO.File]::Open($pending, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
try {
    $username = 'BVT17' + $Nonce.Substring(0, 12)
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    $user = Get-LocalUser -Name $username
    if ($user.Enabled -ne $true -or $user.SID.Value -cne $identity.User.Value) { throw 'not the new fixture account' }
    $task = Get-ScheduledTask -TaskName 'BridgeVM Guest Agent'
    if ($task.Principal.UserId -ine $identity.Name -or $task.Principal.LogonType -ne 'Interactive' -or
        $task.Principal.RunLevel -ne 'Highest' -or $task.State -notin @('Running', 'Ready')) { throw 'fixture task differs' }
    $session = (Get-Process -Id $PID).SessionId
    if ($session -le 0 -or @(Get-Process -Name explorer -ErrorAction SilentlyContinue | Where-Object { $_.SessionId -eq $session }).Count -lt 1) {
        throw 'fixture desktop unavailable'
    }
    # Query only nonsecret Winlogon fields; never load DefaultPassword.
    $key = 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Winlogon'
    $remaining = Get-ItemPropertyValue -LiteralPath $key -Name AutoLogonCount
    $enabled = Get-ItemPropertyValue -LiteralPath $key -Name AutoAdminLogon
    $loginUser = Get-ItemPropertyValue -LiteralPath $key -Name DefaultUserName
    if ($enabled -cne '1' -or $loginUser -ine $username -or $remaining -isnot [int] -or $remaining -lt 1 -or $remaining -gt 4) {
        throw 'fixture automatic logon allowance unavailable'
    }
    Set-LocalUser -Name $username -PasswordNeverExpires $true
    $updated = Get-LocalUser -Name $username
    if ($updated.SID.Value -cne $identity.User.Value -or $null -ne $updated.PasswordExpires) { throw 'fixture expiry policy not verified' }
    $report = [ordered]@{
        schema = 'bridgevm.d11-fixture-ready.v1'; nonce = $Nonce; script_sha256 = $ActualSha256
        account_bound = $true; password_never_expires = $true; agent_task_registered = $true
        desktop_observed = $true; autologon_remaining = $remaining
    }
    $bytes = (New-Object Text.UTF8Encoding($false)).GetBytes(($report | ConvertTo-Json -Compress))
    $stream.Write($bytes, 0, $bytes.Length); $stream.Flush($true)
} finally { $stream.Dispose() }
[IO.File]::Move($pending, $result)
$sha = [Security.Cryptography.SHA256]::Create()
try { $digest = [BitConverter]::ToString($sha.ComputeHash($bytes)).Replace('-', '').ToLowerInvariant() }
finally { $sha.Dispose() }
$completion = [IO.File]::Open($done, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
try {
    $bytes = [Text.Encoding]::ASCII.GetBytes($digest)
    $completion.Write($bytes, 0, $bytes.Length); $completion.Flush($true)
} finally { $completion.Dispose() }
