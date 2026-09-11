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

function Confirm-GuestAgentAssets([object[]]$Records, [string]$Root) {
    $agentHash = $null
    foreach ($name in @('bvagent.ps1', 'bvagent-input.ps1', 'bvagent-unicode-input.cs', 'bvagent-key-input.cs', 'bvagent-task.ps1')) {
        $entries = @($Records | Where-Object { $_.Count -eq 3 -and $_[0] -eq 'guest_tool' -and $_[1] -eq ('agent/' + $name) })
        if ($entries.Count -ne 1 -or $entries[0][2] -notmatch '^[0-9a-f]{64}\z') {
            throw ('guest-payload receipt has no unique asset hash: ' + $name)
        }
        $path = Join-Path $Root ('agent/' + $name)
        if (-not (Test-Path -LiteralPath $path -PathType Leaf)) { throw ('guest asset missing: ' + $name) }
        $hash = (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant()
        if ($hash -cne $entries[0][2]) { throw ('guest asset hash mismatch: ' + $name) }
        if ($name -eq 'bvagent.ps1') { $agentHash = $hash }
    }
    return $agentHash
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
    $actualAgentHash = Confirm-GuestAgentAssets $records $ProvisioningRoot

    . (Join-Path $ProvisioningRoot 'agent/bvagent-task.ps1')
    $identity = Start-VerifiedGuestAgentTask $TaskName $AgentPath

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
