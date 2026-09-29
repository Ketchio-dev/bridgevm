$ErrorActionPreference = 'Stop'
# Runs the committed first-logon provisioner unchanged, as FirstLogonCommands
# does, against a receipt in the exact format stage-hvf-windows-guest-payload.sh
# writes. The task module is a stub, so no scheduled task is ever registered.
$source = Join-Path $PSScriptRoot 'win-assets/bvagent-firstboot.ps1'
$base = 'C:\BridgeVM'
$root = Join-Path $base 'provisioning'
$log = Join-Path $base 'guest-tools-firstboot.log'
$marker = Join-Path $base 'guest-tools-provisioned.json'
foreach ($path in @($root, $log, $marker)) {
    if (Test-Path -LiteralPath $path) { throw ('refusing to replace existing guest state: ' + $path) }
}
$createdBase = -not (Test-Path -LiteralPath $base)
$assets = @('bvagent.ps1', 'bvagent-firstboot.ps1', 'bvagent-input.ps1', 'bvagent-unicode-input.cs', 'bvagent-key-input.cs', 'bvagent-pointer-input.cs', 'bvagent-window-inventory.ps1', 'bv-window-inventory.cs', 'bvagent-task.ps1')
$stub = "function Start-VerifiedGuestAgentTask([string]`$TaskName, [string]`$AgentPath) {`r`n    Set-Content -LiteralPath 'C:\BridgeVM\provisioning\task-started.txt' -Value (`$TaskName + '|' + `$AgentPath)`r`n    return 'BRIDGEVM\contract'`r`n}`r`n"

function Get-Sha256([string]$Path) { (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant() }

function Reset-Provisioning {
    foreach ($path in @($root, $log, $marker)) {
        if (Test-Path -LiteralPath $path) { Remove-Item -LiteralPath $path -Recurse -Force }
    }
    [void](New-Item -ItemType Directory -Path (Join-Path $root 'agent') -Force)
    foreach ($name in $assets) {
        $path = Join-Path $root ('agent\' + $name)
        if ($name -eq 'bvagent-firstboot.ps1') { Copy-Item -LiteralPath $source -Destination $path }
        elseif ($name -eq 'bvagent-task.ps1') { [IO.File]::WriteAllText($path, $stub) }
        else { [IO.File]::WriteAllText($path, 'synthetic asset ' + $name + "`r`n") }
    }
    [IO.File]::WriteAllText((Join-Path $root 'payload-manifest.tsv'), "synthetic manifest`n")
}

function Write-Receipt([string[]]$Header) {
    $lines = @($Header)
    $lines += ('manifest_sha256' + "`t" + (Get-Sha256 (Join-Path $root 'payload-manifest.tsv')))
    $lines += "catalog_signature_policy`tpkcs7-signature-and-digest-only-windows-trust-and-live-bind-still-required"
    $lines += "driver`tvioserial`tvioser.inf`tvioser.cat`tvioser.sys"
    $lines += ("file`tdrivers/vioser.sys`t" + ('0' * 64))
    foreach ($name in $assets) {
        $lines += ('guest_tool' + "`tagent/" + $name + "`t" + (Get-Sha256 (Join-Path $root ('agent\' + $name))))
    }
    [IO.File]::WriteAllText((Join-Path $root 'payload-receipt.tsv'), (($lines -join "`n") + "`n"))
}

function Invoke-Provisioner([string]$Shell) {
    & $Shell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $root 'agent\bvagent-firstboot.ps1') | Out-Null
    $exit = $LASTEXITCODE
    $text = if (Test-Path -LiteralPath $log) { [IO.File]::ReadAllText($log) } else { '' }
    return @{ Exit = $exit; Log = $text }
}

$valid = @("schema`tbridgevm-windows-guest-payload-receipt-v1", "architecture`tarm64")
$refusals = @(
    @{ Name = 'duplicate schema'; Header = @($valid[0], $valid[0], $valid[1]); Expect = 'receipt schema is invalid' },
    @{ Name = 'wrong schema'; Header = @("schema`tbridgevm-windows-guest-payload-receipt-v0", $valid[1]); Expect = 'receipt schema is invalid' },
    @{ Name = 'missing schema'; Header = @($valid[1]); Expect = 'receipt schema is invalid' },
    @{ Name = 'wrong architecture'; Header = @($valid[0], "architecture`tx64"); Expect = 'architecture is not arm64' },
    @{ Name = 'duplicate architecture'; Header = @($valid[0], $valid[1], $valid[1]); Expect = 'architecture is not arm64' }
)
$shells = @('powershell.exe')
if (Get-Command pwsh -ErrorAction SilentlyContinue) { $shells += 'pwsh' }
try {
    foreach ($shell in $shells) {
        Reset-Provisioning
        Write-Receipt $valid
        $result = Invoke-Provisioner $shell
        if ($result.Exit -ne 0 -or $result.Log -notmatch 'BVAGENT PROVISION STAGED') {
            throw ($shell + ': valid receipt was refused (exit ' + $result.Exit + '): ' + $result.Log.Trim())
        }
        $state = Get-Content -LiteralPath $marker -Raw | ConvertFrom-Json
        if ($state.agent_sha256 -cne (Get-Sha256 (Join-Path $root 'agent\bvagent.ps1')) -or $state.interactive_user -cne 'BRIDGEVM\contract') {
            throw ($shell + ': provisioned marker does not bind the verified agent')
        }
        if (-not (Test-Path -LiteralPath (Join-Path $root 'task-started.txt'))) { throw ($shell + ': agent task was not started') }
        foreach ($case in $refusals) {
            Reset-Provisioning
            Write-Receipt $case.Header
            $result = Invoke-Provisioner $shell
            if ($result.Exit -ne 1 -or -not $result.Log.Contains($case.Expect) -or (Test-Path -LiteralPath $marker) -or (Test-Path -LiteralPath (Join-Path $root 'task-started.txt'))) {
                throw ($shell + ': ' + $case.Name + ' was not refused before task start: ' + $result.Log.Trim())
            }
        }
        Reset-Provisioning
        Write-Receipt $valid
        [IO.File]::WriteAllText((Join-Path $root 'agent\bvagent.ps1'), 'mutated')
        $result = Invoke-Provisioner $shell
        if ($result.Exit -ne 1 -or -not $result.Log.Contains('guest asset hash mismatch: bvagent.ps1')) {
            throw ($shell + ': mutated agent was not refused: ' + $result.Log.Trim())
        }
    }
    Write-Output ('Firstboot receipt provisioning: PASS (' + ($shells -join ', ') + '; stub task only)')
} finally {
    foreach ($path in @($root, $log, $marker)) {
        if (Test-Path -LiteralPath $path) { Remove-Item -LiteralPath $path -Recurse -Force }
    }
    if ($createdBase -and (Test-Path -LiteralPath $base) -and -not (Get-ChildItem -LiteralPath $base -Force)) {
        Remove-Item -LiteralPath $base -Force
    }
}
