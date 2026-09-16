# Invoked by the headless contract in its existing isolated sandbox and assertion scope.
$limitState = New-B6FakeState; $limitState.ExitAt = [double]::PositiveInfinity
$limitFactory = New-B6FakeFactory $limitState
$limited = & $limitFactory 'limit' (Join-Path $sandbox 'limit.out') (Join-Path $sandbox 'limit.err')
$limitedWait = Wait-B6TipOwnedChild -Operations $limited.Operations -QueryLimitSeconds 10
Assert-B6 ($limitedWait.TimedOut -and $limitedWait.Elapsed -eq 10 -and $limitedWait.ExitObserved -and $limitedWait.OutputDrained -and $limitState.Kills -eq 1 -and $limitState.WaitMilliseconds -eq 5000) 'Explicit shorter query limit altered cleanup or default policy'
Expect-B6Failure { Wait-B6TipOwnedChild -Operations $limited.Operations -QueryLimitSeconds 0 } 'QueryLimitSeconds'
Expect-B6Failure { Wait-B6TipOwnedChild -Operations $limited.Operations -QueryLimitSeconds 21 } 'QueryLimitSeconds'
& $limited.Dispose

foreach ($cost in @(1.5,6.0)) {
    $state = New-B6FakeState; $state.ExitAt = [double]::PositiveInfinity; $state.KillCost = $cost
    $factory = New-B6FakeFactory $state
    $adapter = & $factory 'cleanup-budget' (Join-Path $sandbox "budget-$cost.out") (Join-Path $sandbox "budget-$cost.err")
    $wait = Wait-B6TipOwnedChild -Operations $adapter.Operations
    $expected = if ($cost -lt 5) { 3500 } else { 0 }
    Assert-B6 ($state.WaitMilliseconds -eq $expected -and $wait.ExitObserved -eq ($cost -lt 5)) 'Cleanup granted a new five seconds after earlier cleanup work'
}

foreach ($afterWait in @($false,$true)) {
    $state = @{ Time = 0.0; AfterWait = $afterWait; Waited = $false; Kills = 0 }
    $ops = @{
        Elapsed = { $state.Time }.GetNewClosure()
        Observe = { if (!$state.AfterWait -or $state.Waited) { $state.Time += 6 }; @{ Exited = $true; Code = 17; StreamsDrained = (!$state.AfterWait -or $state.Waited) } }.GetNewClosure()
        Kill = { $state.Kills++ }.GetNewClosure()
        WaitExit = { param($milliseconds) $state.Waited = $true; $true }.GetNewClosure()
    }
    $wait = Wait-B6TipOwnedChild -Operations $ops -InitialFailure 'original failure'
    Assert-B6 ($wait.ExitObserved -and $wait.Code -eq 17 -and !$wait.OutputDrained -and $state.Kills -eq 0 -and $wait.CleanupError -match 'deadline expired' -and $wait.PrimaryError -eq 'original failure') 'Late cleanup observation became proof or erased the original failure'
}

foreach ($kind in @('delayed', 'stalled', 'faulted', 'late-drain')) {
    $run = New-TestRun; $state = New-B6FakeState; $state.DrainAt = 10
    if ($kind -eq 'stalled') { $state.DrainAt = [double]::PositiveInfinity }
    if ($kind -eq 'faulted') { $state.DrainFails = $true }
    if ($kind -eq 'late-drain') { $state.DrainAt = 21; $state.DrainWaitFinishes = $true }
    if ($kind -eq 'delayed') {
        $result = Invoke-B6TipQuery $run 'fixture.ps1' 42 'visible-owned' -Factory (New-B6FakeFactory $state)
        Assert-B6 ($state.Time -eq 10 -and $result.Output -eq 'fixture-out') 'Child exit bypassed pending output'
    } else {
        $pattern = 'output cleanup unconfirmed'; if ($kind -eq 'late-drain') { $pattern = 'timed out' }
        Expect-B6Failure { Invoke-B6TipQuery $run 'fixture.ps1' 42 'visible-owned' -Factory (New-B6FakeFactory $state) } $pattern
        Assert-B6 ($state.Kills -eq 0 -and $state.WaitMilliseconds -eq 5000) 'Exited child drain altered kill or shared cleanup bound'
    }
    Assert-B6 ($run.Cases[0].exit_observed -and $run.Cases[0].exit_code -eq 0) 'Drain state erased retained child exit or code'
    if ($kind -in @('stalled','faulted')) {
        Assert-B6 (!$run.RawCleanupSafe -and !$run.Cases[0].output_drained -and $state.Disposed -eq 0 -and $run.RetainedOwners.Count -eq 1) 'Unresolved stream owner was disposed or declared safe'
        Assert-B6 ($run.Cases[0].streams.Count -eq 0) 'Unresolved output was captured'
        if ($kind -eq 'faulted') { Assert-B6 ($run.Cases[0].primary_error -match 'stdout read fixture failure') 'Lost original drain failure' }
        Expect-B6Failure { Read-B6TipSnapshot $run $run.Cases[0] (Join-Path $run.RawRoot '1-visible-owned.stdout.raw') 'stdout' } 'stream drain'
    } else { Assert-B6 ($run.RawCleanupSafe -and $run.Cases[0].output_drained -and $state.Disposed -eq 1) 'Drained owner was not released' }
    Complete-B6TipEvidenceRun $run ($kind -ne 'delayed')
    if ($kind -in @('stalled','faulted')) { Assert-B6 ((Test-Path $run.RawRoot) -and @([IO.Directory]::GetFiles($run.PublishRoot, '*.txt')).Count -eq 0) 'Unresolved raw output was removed or published' }
}

foreach ($kind in @('eof','read-fault','flush-fault')) {
    $path = Join-Path $sandbox "$kind.raw"; $stream = New-B6TipOutputStream $path
    $stream.Reader = [IO.MemoryStream]::new(); $completion = [Threading.Tasks.TaskCompletionSource[int]]::new()
    $stream.Pending = $completion.Task
    try {
        Receive-B6TipOutput $stream
        Assert-B6 (!$stream.EOF -and !$stream.Closed -and $stream.Pending -eq $completion.Task) 'Pending read was discarded or declared EOF'
        Expect-B6Failure { $probe = [IO.File]::Open($path, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read); $probe.Dispose() } '.'
        if ($kind -eq 'read-fault') { $completion.SetException([IO.IOException]::new('injected read failure')) }
        else { if ($kind -eq 'flush-fault') { $stream.Writer.Dispose() }; $completion.SetResult(0) }
        Receive-B6TipOutput $stream
        if ($kind -eq 'eof') {
            $probe = [IO.File]::Open($path, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read); $probe.Dispose()
            Assert-B6 ($stream.EOF -and $stream.Closed -and !$stream.Writer -and !$stream.Pending) 'EOF did not close the exact writer before capture'
        } elseif ($kind -eq 'read-fault') { Assert-B6 (!$stream.EOF -and !$stream.Closed -and $stream.Error -match 'injected read failure') 'Read failure became successful drain' }
        else { Assert-B6 ($stream.EOF -and !$stream.Closed -and $stream.Error) 'EOF without successful flush/close became drain proof' }
    } finally { if ($stream.Writer) { $stream.Writer.Dispose() }; $stream.Reader.Dispose() }
}

$pendingStreams = @(); $completions = @()
try {
    foreach ($name in @('held-out','held-err')) {
        $stream = New-B6TipOutputStream (Join-Path $sandbox "$name.raw")
        $stream.Reader = [IO.MemoryStream]::new(); $completion = [Threading.Tasks.TaskCompletionSource[int]]::new()
        $stream.Pending = $completion.Task; $pendingStreams += $stream; $completions += $completion
    }
    $owned = @{ Process = [pscustomobject]@{ HasExited = $true; ExitCode = 0 }; Streams = $pendingStreams; Receive = ${function:Receive-B6TipOutput}; Observe = ${function:Get-B6TipProcessObservation} }
    $clock = [Diagnostics.Stopwatch]::StartNew()
    $finished = Wait-B6TipProcessCompletion $owned 50
    Assert-B6 (!$finished -and $clock.Elapsed.TotalSeconds -lt 1) 'Two unresolved streams exceeded one bounded cleanup wait'
    Assert-B6 (@($pendingStreams | Where-Object { $_.Closed -or !$_.Pending -or $_.EOF }).Count -eq 0) 'Bounded cleanup abandoned a pending task or writer'
} finally {
    foreach ($completion in $completions) { $completion.SetResult(0) }
    foreach ($stream in $pendingStreams) { Receive-B6TipOutput $stream; if ($stream.Writer) { $stream.Writer.Dispose() }; $stream.Reader.Dispose() }
}

foreach ($size in @(65536,65537,100000)) {
    $path = Join-Path $sandbox "cap-$size.raw"; $stream = New-B6TipOutputStream $path
    $bytes = [Text.Encoding]::UTF8.GetBytes(('x' * $size)); $stream.Reader = [IO.MemoryStream]::new($bytes)
    while (!$stream.Closed -and !$stream.Error) { Receive-B6TipOutput $stream }
    Assert-B6 ($stream.EOF -and $stream.Closed -and $stream.SourceBytes -eq $size -and ([IO.FileInfo]$path).Length -eq [Math]::Min($size,65537)) 'Owned output cap lost total count or exceeded disk bound'
}

foreach ($size in @(65536,65537)) {
    $run = New-TestRun; $state = New-B6FakeState
    $state.OutBytes = [Text.Encoding]::UTF8.GetBytes(('x' * ($size - 2)) + [char]0x00E9)
    if ($size -eq 65536) { $null = Invoke-B6TipQuery $run 'fixture.ps1' 42 'visible-owned' -Factory (New-B6FakeFactory $state) }
    else { Expect-B6Failure { Invoke-B6TipQuery $run 'fixture.ps1' 42 'visible-owned' -Factory (New-B6FakeFactory $state) } 'capture refused' }
    Assert-B6 ($run.Cases[0].streams.stdout.captured_bytes -eq 65536 -and $run.Cases[0].streams.stdout.truncated -eq ($size -gt 65536)) 'UTF8 boundary changed bounded capture/refusal'
    Complete-B6TipEvidenceRun $run ($size -gt 65536)
}

foreach ($iteration in 1..4) {
    $query = Join-Path $sandbox 'stream tail fixture.ps1'; $code = if ($iteration % 2) { 0 } else { 7 }
    [IO.File]::WriteAllText($query, "[Console]::Out.Write(('o' * 20000) + 'OUT-TAIL'); [Console]::Error.Write(('e' * 20000) + 'ERR-TAIL'); exit $code")
    $run = New-TestRun -Real; $result = Invoke-B6TipQuery $run $query 42 'visible-owned'
    Assert-B6 ($result.Code -eq $code -and $result.Output -eq (('o' * 20000) + 'OUT-TAIL') -and $result.Error -eq (('e' * 20000) + 'ERR-TAIL')) 'Real concurrent output lost a tail, argument boundary or exit code'
    Assert-B6 ($run.Cases[0].output_drained -and $run.Cases[0].output.stdout.writer_closed -and $run.Cases[0].output.stderr.writer_closed) 'Real result preceded writer closure'
    Complete-B6TipEvidenceRun $run $false
}

$query = Join-Path $sandbox 'byte encoding fixture.ps1'
[IO.File]::WriteAllText($query, @'
$out = [byte[]]@(239,187,191,237,149,156); $err = [byte[]]@(255,254,0,174)
[Console]::OpenStandardOutput().Write($out, 0, $out.Length)
[Console]::OpenStandardError().Write($err, 0, $err.Length)
'@)
$run = New-TestRun -Real; $result = Invoke-B6TipQuery $run $query 42 'visible-owned'
Assert-B6 ($result.Output -eq [string][char]0xD55C -and $result.Error -eq [string][char]0xAE00) 'Raw drain changed UTF8/UTF16 BOM decoding'
$raw = [IO.File]::ReadAllBytes((Join-Path $run.RawRoot '1-visible-owned.stdout.raw'))
Assert-B6 ([Convert]::ToBase64String($raw) -eq [Convert]::ToBase64String([byte[]]@(239,187,191,237,149,156))) 'Owned writer re-encoded stdout bytes'
Complete-B6TipEvidenceRun $run $false

[IO.File]::WriteAllText($query, "[Console]::Out.Write('z' * 100000)")
$run = New-TestRun -Real
Expect-B6Failure { Invoke-B6TipQuery $run $query 42 'visible-owned' } 'capture refused'
Assert-B6 ($run.RawCleanupSafe -and $run.Cases[0].output_drained -and $run.Cases[0].output.stdout.stored_bytes -eq 65537 -and $run.Cases[0].streams.stdout.source_bytes -eq 100000 -and $run.Cases[0].streams.stdout.truncated) 'Real overflow lost source count, bound, refusal or drain proof'
Complete-B6TipEvidenceRun $run $true

$run = New-TestRun
$disposeFactory = {
    param($arguments, $stdout, $stderr)
    $adapter = & (New-B6FakeFactory (New-B6FakeState)) $arguments $stdout $stderr
    $adapter.Dispose = { throw 'injected owner disposal failure' }; return $adapter
}
Expect-B6Failure { Invoke-B6TipQuery $run 'fixture.ps1' 42 'visible-owned' -Factory $disposeFactory } 'owner disposal failure'
Assert-B6 (!$run.RawCleanupSafe -and $run.RetainedOwners.Count -eq 1 -and $run.Cases[0].dispose_error -match 'owner disposal failure') 'Disposal failure lost ownership or permitted raw cleanup'
Complete-B6TipEvidenceRun $run $true

$heldBefore = $global:BridgeVMB6RetainedOutputRuns.Count
$weak = & {
    $localRun = New-B6TipEvidenceRun -EvidenceRoot (Join-Path $sandbox 'retained-scope')
    $localRun.RawCleanupSafe = $false
    $localRun.RetainedOwners.Add([pscustomobject]@{ Kind = 'synthetic retained owner' })
    Complete-B6TipEvidenceRun $localRun $false
    Complete-B6TipEvidenceRun $localRun $false
    return [WeakReference]::new($localRun)
}
[GC]::Collect(); [GC]::WaitForPendingFinalizers(); [GC]::Collect()
Assert-B6 ($weak.IsAlive -and $global:BridgeVMB6RetainedOutputRuns.Count -eq $heldBefore + 1) 'Unresolved run was lost at script scope exit or registered twice'
$retained = $weak.Target
Assert-B6 ($retained.RetainedOwners.Count -eq 1 -and (Test-Path $retained.RawRoot)) 'Process-lifetime retention lost owner or raw files'
$retained.RawCleanupSafe = $true # Synthetic owner only: no process, task or writer was created here.
Complete-B6TipEvidenceRun $retained $false
Assert-B6 ($global:BridgeVMB6RetainedOutputRuns.Count -eq $heldBefore -and !(Test-Path $retained.RawRoot)) 'Confirmed synthetic cleanup did not release its retained run'

foreach ($kind in @('none-created','partial-created','rollback-failed')) {
    $creation = @{ Kind = $kind; Path = $null }
    function New-Item {
        [CmdletBinding()]param([string]$ItemType, [string]$Path)
        $creation.Path = $Path
        if ($creation.Kind -ne 'none-created') { $null = [IO.Directory]::CreateDirectory($Path) }
        throw 'injected directory creation failure'
    }
    function Remove-Item {
        [CmdletBinding()]param([string]$LiteralPath, [switch]$Recurse, [switch]$Force)
        if ($creation.Kind -eq 'rollback-failed') { throw 'injected directory rollback failure' }
        Microsoft.PowerShell.Management\Remove-Item -LiteralPath $LiteralPath -Recurse:$Recurse -Force:$Force
    }
    try { Expect-B6Failure { New-B6TipEvidenceRun -EvidenceRoot (Join-Path $sandbox 'constructor-failure') } 'directory creation failure' }
    finally { Microsoft.PowerShell.Management\Remove-Item Function:New-Item; Microsoft.PowerShell.Management\Remove-Item Function:Remove-Item }
    $raw = Split-Path $creation.Path
    if ($kind -eq 'rollback-failed') {
        $retained = $global:BridgeVMB6RetainedOutputRuns[$heldBefore]
        Assert-B6 ($global:BridgeVMB6RetainedOutputRuns.Count -eq $heldBefore + 1 -and !$retained.RawCleanupSafe -and $retained.RawRoot -eq $raw -and (Test-Path $raw)) 'Uncertain constructor rollback lost its reserved run'
        Assert-B6 ($retained.InitializationError -match 'creation failure' -and $retained.InitializationCleanupError -match 'rollback failure') 'Constructor failure evidence was overwritten'
        $retained.RawCleanupSafe = $true # Injected directory-only failure; no child, pipe, task or writer exists.
        Complete-B6TipEvidenceRun $retained $false
    }
    Assert-B6 ($global:BridgeVMB6RetainedOutputRuns.Count -eq $heldBefore -and !(Test-Path $raw)) 'Known-safe constructor rollback leaked its directory or reservation'
}

$reservations = [Collections.Generic.List[object]]::new()
try {
    while ($global:BridgeVMB6RetainedOutputRuns.Count -lt 16) {
        $reservation = @{ RawCleanupSafe = $false; TestOwnsNoOSResources = $true }
        Register-B6TipOutputRun $reservation; $reservations.Add($reservation)
    }
    Register-B6TipOutputRun $reservations[0]
    Expect-B6Failure { New-B6TipEvidenceRun -EvidenceRoot (Join-Path $sandbox 'capacity-refusal') } 'capacity reached'
    Assert-B6 ($global:BridgeVMB6RetainedOutputRuns.Count -eq 16 -and $global:BridgeVMB6RetainedOutputRuns.Contains($reservations[0])) 'Capacity refusal evicted an unresolved owner or duplicated registration'
} finally {
    foreach ($reservation in $reservations) { $reservation.RawCleanupSafe = $true; Set-B6TipOutputRetention $reservation }
}
Assert-B6 ($global:BridgeVMB6RetainedOutputRuns.Count -eq $heldBefore) 'Synthetic capacity test leaked a reservation'
