$ErrorActionPreference = 'Stop'
$BridgeVMRoot = 'C:\BridgeVM'
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
    foreach ($name in @('bvagent.ps1', 'bvagent-input.ps1', 'bvagent-unicode-input.cs', 'bvagent-key-input.cs', 'bvagent-pointer-input.cs', 'bvagent-window-inventory.ps1', 'bv-window-inventory.cs', 'bvagent-task.ps1')) {
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

# The logon task runs the agent from C:\BridgeVM\provisioning\agent elevated for the VM's account
# (unattend creates it as an Administrator), but C:\BridgeVM inherits C:\'s defaults, under which
# every signed-in user may change, rename or replace anything below it. So before anything there is
# read or run, C:\BridgeVM gets its own permissions, owned by Administrators: SYSTEM and
# Administrators full control, Users read and run (user programs read files such as a Vulkan ICD
# manifest), nothing inherited; everything below it is reset to inherit just that. C:\BridgeVM is
# locked first: from then on no one else can add, rename or replace anything below it, so the scan
# for reparse points (junctions or links, which a recursive reset would follow out of the folder)
# cannot be raced. If the scan finds one, provisioning stops with C:\BridgeVM locked and the
# children not reset, which is safe: only SYSTEM and Administrators can change anything there.
# SIDs, not names: Windows ships localized. On a product install only WinPE, this script and the
# elevated agent write below C:\BridgeVM; the agent's own log is C:\bvagent.log.
function Lock-BridgeVMFolder([string]$Root) {
    try {
        $sid = [Security.Principal.SecurityIdentifier]
        $reparse = [IO.FileAttributes]::ReparsePoint
        if ((Get-Item -LiteralPath $Root -Force).Attributes -band $reparse) { throw ($Root + ' is a reparse point (a junction or link)') }
        $acl = Get-Acl -LiteralPath $Root
        $acl.SetSecurityDescriptorSddlForm('O:S-1-5-32-544D:PAI(A;OICI;FA;;;S-1-5-18)(A;OICI;FA;;;S-1-5-32-544)(A;OICI;0x1200a9;;;S-1-5-32-545)', 'Owner, Access')
        $locked = $acl.GetSecurityDescriptorSddlForm('Owner, Access')
        Set-Acl -LiteralPath $Root -AclObject $acl
        $items = @(Get-ChildItem -LiteralPath $Root -Recurse -Force)
        foreach ($item in $items) {
            if ($item.Attributes -band $reparse) { throw ($item.FullName + ' is a reparse point (a junction or link)') }
        }
        foreach ($item in $items) {
            $acl = Get-Acl -LiteralPath $item.FullName
            $acl.SetSecurityDescriptorSddlForm('O:S-1-5-32-544D:AI', 'Owner, Access')
            Set-Acl -LiteralPath $item.FullName -AclObject $acl
        }
        $actual = (Get-Acl -LiteralPath $Root).GetSecurityDescriptorSddlForm('Owner, Access')
        if ($actual -cne $locked) { throw ('the folder reads back as ' + $actual) }
        foreach ($item in $items) {
            $acl = Get-Acl -LiteralPath $item.FullName
            $rules = @($acl.GetAccessRules($true, $true, $sid) | ForEach-Object { '{0} {1} {2} {3}' -f $_.IdentityReference, $_.AccessControlType, $_.FileSystemRights, $_.IsInherited } | Sort-Object) -join '; '
            if ($acl.AreAccessRulesProtected -or $acl.GetOwner($sid).Value -cne 'S-1-5-32-544' -or $rules -cne 'S-1-5-18 Allow FullControl True; S-1-5-32-544 Allow FullControl True; S-1-5-32-545 Allow ReadAndExecute, Synchronize True') {
                throw ($item.FullName + ' does not just inherit the folder (' + $rules + ')')
            }
        }
    } catch {
        throw ($Root + ' could not be locked so that only SYSTEM and Administrators can change it: ' + $_.Exception.Message)
    }
}

try {
    Write-ProvisionLog 'BVAGENT PROVISION START'
    if (-not (Test-Path -LiteralPath $ReceiptPath -PathType Leaf)) {
        throw 'sealed guest-payload receipt is missing'
    }
    if (-not (Test-Path -LiteralPath $AgentPath -PathType Leaf)) {
        throw 'BridgeVM agent script is missing'
    }
    Lock-BridgeVMFolder $BridgeVMRoot

    $records = Get-Content -LiteralPath $ReceiptPath | ForEach-Object { ,$_.Split([char]9) }
    $schema = @($records | Where-Object { $_.Count -eq 2 -and $_[0] -eq 'schema' })
    if ($schema.Count -ne 1 -or $schema[0][1] -ne 'bridgevm-windows-guest-payload-receipt-v1') {
        throw 'guest-payload receipt schema is invalid'
    }
    $architecture = @($records | Where-Object { $_.Count -eq 2 -and $_[0] -eq 'architecture' })
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
