$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'window-inventory-error-child.ps1')
$sandbox = Join-Path ([IO.Path]::GetTempPath()) ('inventory-owned-contract-' + [guid]::NewGuid().ToString('N'))
$fixtures = [Collections.Generic.List[object]]::new(); $script:inventoryChecks = 0
New-Item -ItemType Directory $sandbox | Out-Null
$sentinel = New-B6TipEvidenceRun -EvidenceRoot (Join-Path $sandbox 'unrelated-run'); $contractFailed = $true
function Assert-Inventory([bool]$Condition, [string]$Message) {
    if (!$Condition) { throw $Message }; $script:inventoryChecks++
}
function Expect-InventoryFailure([scriptblock]$Action, [string]$Pattern) {
    $message = $null
    try { & $Action | Out-Null } catch { $message = $_.Exception.Message }
    Assert-Inventory ($message -and $message -match $Pattern) "Expected failure matching $Pattern; observed $message"
}
function New-InventoryFixture([string]$Mode = 'timely', [switch]$Real) {
    $root = Join-Path $sandbox ([guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory $root | Out-Null
    $run = New-B6TipEvidenceRun -EvidenceRoot (Join-Path $sandbox 'published')
    $state = @{ Time = 1.0; Exited = $true; Drained = $true; Code = 1; Kills = 0; Disposed = 0
        Waits = [Collections.Generic.List[int]]::new(); Output = ''; Error = 'fixture provider failure'; Mode = $Mode }
    if ($Mode -in @('deadline','late-cleanup','cleanup-deadline','unconfirmed-exit','unconfirmed-drain')) { $state.Time = 10.0 }
    if ($Mode -in @('late-cleanup','cleanup-deadline','unconfirmed-exit')) { $state.Exited = $false }
    if ($Mode -in @('late-cleanup','cleanup-deadline','unconfirmed-drain')) { $state.Drained = $false }
    if ($Mode -eq 'cross-deadline') { $state.Time = 9.9 }
    $operations = @{
        Elapsed = { $state.Time }.GetNewClosure()
        Observe = {
            if ($state.Mode -eq 'cross-deadline') { $state.Time = 10.0 }
            @{ Exited = $state.Exited; StreamsDrained = $state.Drained; Code = $state.Code; StreamError = '' }
        }.GetNewClosure()
        Kill = { $state.Kills++ }.GetNewClosure()
        WaitExit = {
            param($milliseconds)
            $state.Waits.Add($milliseconds)
            if ($state.Mode -eq 'late-cleanup') { $state.Exited = $true; $state.Drained = $true; $state.Time = 12.0 }
            if ($state.Mode -eq 'cleanup-deadline') { $state.Exited = $true; $state.Drained = $true; $state.Time = 15.0 }
            return $state.Exited -and $state.Drained
        }.GetNewClosure()
        Pause = { throw 'Unexpected fixture pause' }
    }
    $dispose = { $state.Disposed++ }.GetNewClosure()
    $factory = {
        param($arguments, $stdout, $stderr)
        [IO.File]::WriteAllText($stdout, $state.Output, [Text.UTF8Encoding]::new($false))
        [IO.File]::WriteAllText($stderr, $state.Error, [Text.UTF8Encoding]::new($false))
        @{ Owned = $true; AdmissionError = $null; Operations = $operations; Owner = @{ PID = 17; Started = 'synthetic' }
            Dispose = $dispose }
    }.GetNewClosure()
    $fixture = @{ Run = $run; Root = $root; State = $state; Factory = $factory; Operations = $operations; Real = [bool]$Real }
    $fixtures.Add($fixture)
    return $fixture
}
function Invoke-InventoryFixture($Fixture) {
    Invoke-BvInventoryErrorChild -Run $Fixture.Run -Name native_failure -Script (Join-Path $Fixture.Root 'synthetic.ps1') -Factory $Fixture.Factory
}
try {
    $fixture = New-InventoryFixture 'deadline'
    $twenty = Wait-B6TipOwnedChild -Operations $fixture.Operations -QueryLimitSeconds 20
    $default = Wait-B6TipOwnedChild -Operations $fixture.Operations
    Assert-Inventory (!$twenty.TimedOut -and !$default.TimedOut -and $twenty.ExitObserved -and $twenty.OutputDrained) 'Default B6 20-second admission changed'
    Expect-InventoryFailure { Invoke-InventoryFixture $fixture } 'native_failure child deadline'
    Assert-Inventory ($fixture.Run.Cases[0].timed_out -and $fixture.State.Disposed -eq 1) 'Inventory deadline10 became deadline20 or lost confirmed cleanup'
    foreach ($mode in @('cross-deadline','late-cleanup')) {
        $fixture = New-InventoryFixture $mode
        Expect-InventoryFailure { Invoke-InventoryFixture $fixture } 'native_failure child deadline'
        Assert-Inventory ($fixture.Run.RawCleanupSafe -and $fixture.State.Disposed -eq 1 -and $fixture.Run.Cases[0].timed_out) 'Late cleanup erased original deadline or abandoned confirmed owner'
        if ($mode -eq 'late-cleanup') {
            Assert-Inventory ($fixture.State.Kills -eq 1 -and $fixture.State.Waits.Count -eq 1 -and $fixture.State.Waits[0] -gt 0 -and $fixture.State.Waits[0] -le 5000) 'Cleanup did not use one retained-child kill and one bounded5-second wait'
        }
    }
    foreach ($mode in @('unconfirmed-exit','unconfirmed-drain','cleanup-deadline')) {
        $fixture = New-InventoryFixture $mode
        Expect-InventoryFailure { Invoke-InventoryFixture $fixture } 'unconfirmed'
        Assert-Inventory (!$fixture.Run.RawCleanupSafe -and $fixture.State.Disposed -eq 0 -and $fixture.Run.RetainedOwners.Count -eq 1) 'Unconfirmed owner was disposed or lost'
        Assert-Inventory ($fixture.Run.Cases[0].streams.Count -eq 0 -and $fixture.State.Waits.Count -eq 1 -and $fixture.State.Waits[0] -gt 0 -and $fixture.State.Waits[0] -le 5000) 'Unconfirmed output was read or cleanup budget repeated'
        Assert-Inventory ($fixture.State.Kills -eq [int]($mode -ne 'unconfirmed-drain')) 'Kill ignored exact retained exit observation'
        Complete-BvInventoryErrorRun $fixture.Run $fixture.Root $true
        Assert-Inventory ((Test-Path -LiteralPath $fixture.Root) -and (Test-Path -LiteralPath $fixture.Run.RawRoot) -and $global:BridgeVMB6RetainedOutputRuns.Contains($fixture.Run)) 'Unconfirmed fixture, raw output or strong owner was discarded'
        $held = $global:BridgeVMB6RetainedOutputRuns.Count
        Complete-BvInventoryErrorRun $fixture.Run $fixture.Root $false
        Assert-Inventory ($global:BridgeVMB6RetainedOutputRuns.Count -eq $held -and $global:BridgeVMB6RetainedOutputRuns.Contains($fixture.Run)) 'Repeated unresolved completion duplicated or removed its retained run'
        if ($mode -eq 'unconfirmed-drain') {
            $originalFailure = $fixture.Run.Cases[0].failure
            $fixture.State.Drained = $true # Resolve only this resource-free fake's pending drain.
            $confirmed = Wait-B6TipOwnedChild -Operations $fixture.Operations -InitialFailure 'preserved fixture failure' -QueryLimitSeconds 10
            Assert-Inventory ($confirmed.ExitObserved -and $confirmed.OutputDrained) 'Synthetic later cleanup was not confirmed'
            & $fixture.Run.RetainedOwners[0].Dispose
            $fixture.Run.RawCleanupSafe = $confirmed.ExitObserved -and $confirmed.OutputDrained
            Complete-BvInventoryErrorRun $fixture.Run $fixture.Root $false
            Assert-Inventory (!$global:BridgeVMB6RetainedOutputRuns.Contains($fixture.Run) -and !(Test-Path -LiteralPath $fixture.Root) -and !(Test-Path -LiteralPath $fixture.Run.RawRoot) -and $fixture.Run.Cases[0].failure -ceq $originalFailure) 'Confirmed release retained its owner or rewrote the original failure'
        }
    }
    foreach ($case in @(
        @{ Code = 1; Output = ''; Error = 'failure'; Accepted = $true },
        @{ Code = -1; Output = ''; Error = ' '; Accepted = $true },
        @{ Code = 0; Output = ''; Error = 'failure'; Accepted = $false },
        @{ Code = 1; Output = 'UNREACHED'; Error = 'failure'; Accepted = $false },
        @{ Code = 1; Output = ''; Error = ''; Accepted = $false })) {
        $fixture = New-InventoryFixture
        $fixture.State.Code = $case.Code; $fixture.State.Output = $case.Output; $fixture.State.Error = $case.Error
        if ($case.Accepted) {
            $result = Invoke-InventoryFixture $fixture
            Assert-Inventory ($result.Code -eq $case.Code -and $result.Output -ceq $case.Output -and $result.Error -ceq $case.Error) 'Accepted inventory result changed'
        } else { Expect-InventoryFailure { Invoke-InventoryFixture $fixture } 'did not fail closed under unguarded Continue' }
        Assert-Inventory ($fixture.Run.RawCleanupSafe -and $fixture.State.Disposed -eq 1) 'Predicate failure changed owned cleanup'
    }
    foreach ($size in @(65536,65537)) {
        $fixture = New-InventoryFixture -Real
        $path = Join-Path $fixture.Root 'harmless-output.ps1'
        [IO.File]::WriteAllText($path, "[Console]::Error.Write(('x' * $size)); exit 1", [Text.UTF8Encoding]::new($false))
        $action = { Invoke-BvInventoryErrorChild -Run $fixture.Run -Name native_failure -Script $path }
        if ($size -eq 65536) {
            $result = & $action
            Assert-Inventory ($result.Code -eq 1 -and $result.Output.Length -eq 0 -and $result.Error.Length -eq 65536) 'Concurrent raw drain lost exact output larger than pipe capacity'
        } else { Expect-InventoryFailure $action '65536-byte acceptance limit' }
        Assert-Inventory ($fixture.Run.RawCleanupSafe -and $fixture.Run.Cases[0].exit_observed -and $fixture.Run.Cases[0].output_drained) 'Real harmless child/output cleanup unconfirmed'
        Assert-Inventory ($fixture.Run.Cases[0].output.stderr.source_bytes -eq $size -and $fixture.Run.Cases[0].output.stderr.stored_bytes -eq $size) 'Raw drain source count or bounded storage changed'
    }
    $contractFailed = $false
} finally {
    $actualUnconfirmed = $false
    foreach ($fixture in $fixtures) {
        if ($fixture.Real -and !$fixture.Run.RawCleanupSafe) {
            $actualUnconfirmed = $true
            Complete-BvInventoryErrorRun $fixture.Run $fixture.Root $true
            continue
        }
        if ($fixture.Real) {
            Complete-BvInventoryErrorRun $fixture.Run $fixture.Root $contractFailed
            Assert-Inventory (!$global:BridgeVMB6RetainedOutputRuns.Contains($fixture.Run) -and !(Test-Path -LiteralPath $fixture.Run.RawRoot) -and !(Test-Path -LiteralPath $fixture.Root)) 'Confirmed real fixture did not release through the normal finalizer'
            continue
        }
        # Fake adapters own no process, stream task or OS writer; remove their synthetic retained records only.
        $null = $global:BridgeVMB6RetainedOutputRuns.Remove($fixture.Run)
        if (Test-Path -LiteralPath $fixture.Run.RawRoot) { Remove-Item -LiteralPath $fixture.Run.RawRoot -Recurse -Force }
    }
    Assert-Inventory ($global:BridgeVMB6RetainedOutputRuns.Contains($sentinel)) 'Fixture cleanup removed an unrelated reserved run'
    Complete-B6TipEvidenceRun $sentinel $false
    Assert-Inventory (!$global:BridgeVMB6RetainedOutputRuns.Contains($sentinel)) 'Unrelated confirmed reservation was not releasable'
    if (!$actualUnconfirmed) { Remove-Item -LiteralPath $sandbox -Recurse -Force }
    else { Write-Host "Real inventory fixture ownership unconfirmed; contract directory retained at $sandbox" }
}
Write-Output "PASS: $script:inventoryChecks inventory child lifecycle assertions; deadline10 retained; synthetic processes only"
