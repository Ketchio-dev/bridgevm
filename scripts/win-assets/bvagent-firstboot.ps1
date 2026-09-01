$ErrorActionPreference = 'Stop'
$ProvisioningRoot = 'C:\BridgeVM\provisioning'
$ReceiptPath = Join-Path $ProvisioningRoot 'payload-receipt.tsv'
$AgentPath = Join-Path $ProvisioningRoot 'agent\bvagent.ps1'
$LogPath = 'C:\BridgeVM\guest-tools-firstboot.log'
$MarkerPath = 'C:\BridgeVM\guest-tools-provisioned.json'
$TaskName = 'BridgeVM Guest Agent'

function Write-ProvisionLog([string]$Message) {
    $line = ('{0:o} {1}' -f [DateTime]::UtcNow, $Message)
    Add-Content -LiteralPath $LogPath -Value $line -Encoding UTF8
}

try {
    Write-ProvisionLog 'BVAGENT PROVISION START'
    if (-not (Test-Path -LiteralPath $ReceiptPath -PathType Leaf)) {
        throw 'sealed guest-payload receipt is missing'
    }
    if (-not (Test-Path -LiteralPath $AgentPath -PathType Leaf)) {
        throw 'BridgeVM agent script is missing'
    }

    $records = Get-Content -LiteralPath $ReceiptPath | ForEach-Object { ,$_.Split([char]9) }
    $schema = $records | Where-Object { $_.Count -eq 2 -and $_[0] -eq 'schema' }
    if ($schema.Count -ne 1 -or $schema[0][1] -ne 'bridgevm-windows-guest-payload-receipt-v1') {
        throw 'guest-payload receipt schema is invalid'
    }
    $architecture = $records | Where-Object { $_.Count -eq 2 -and $_[0] -eq 'architecture' }
    if ($architecture.Count -ne 1 -or $architecture[0][1] -ne 'arm64') {
        throw 'guest-payload receipt architecture is not arm64'
    }
    $agentRecord = $records | Where-Object {
        $_.Count -eq 3 -and $_[0] -eq 'guest_tool' -and $_[1] -eq 'agent/bvagent.ps1'
    }
    if ($agentRecord.Count -ne 1 -or $agentRecord[0][2] -notmatch '^[0-9a-f]{64}$') {
        throw 'guest-payload receipt has no unique BridgeVM agent hash'
    }
    $actualAgentHash = (Get-FileHash -LiteralPath $AgentPath -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($actualAgentHash -ne $agentRecord[0][2]) {
        throw 'BridgeVM agent hash does not match the sealed receipt'
    }

    $identity = [Security.Principal.WindowsIdentity]::GetCurrent().Name
    if ([string]::IsNullOrWhiteSpace($identity) -or $identity.EndsWith('$')) {
        throw 'first-logon interactive user identity is unavailable'
    }
    $arguments = '-NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -File "' + $AgentPath + '"'
    $action = New-ScheduledTaskAction -Execute 'powershell.exe' -Argument $arguments
    $trigger = New-ScheduledTaskTrigger -AtLogOn -User $identity
    $principal = New-ScheduledTaskPrincipal -UserId $identity -LogonType Interactive -RunLevel Highest
    $settings = New-ScheduledTaskSettingsSet -ExecutionTimeLimit ([TimeSpan]::Zero) -RestartCount 3 -RestartInterval (New-TimeSpan -Minutes 1)
    $task = New-ScheduledTask -Action $action -Trigger $trigger -Principal $principal -Settings $settings
    Register-ScheduledTask -TaskName $TaskName -InputObject $task -Force | Out-Null
    Start-ScheduledTask -TaskName $TaskName

    $receiptHash = (Get-FileHash -LiteralPath $ReceiptPath -Algorithm SHA256).Hash.ToLowerInvariant()
    [ordered]@{
        schema_version = 1
        task = $TaskName
        interactive_user = $identity
        agent_sha256 = $actualAgentHash
        payload_receipt_sha256 = $receiptHash
        provisioned_at_utc = [DateTime]::UtcNow.ToString('o')
        live_agent_ready_proven = $false
    } | ConvertTo-Json | Set-Content -LiteralPath $MarkerPath -Encoding UTF8
    Write-ProvisionLog 'BVAGENT PROVISION STAGED; waiting for BVAGENT READY on the virtio-serial channel'
    exit 0
} catch {
    Write-ProvisionLog ('BVAGENT PROVISION BLOCKED: ' + $_.Exception.Message)
    exit 1
}
