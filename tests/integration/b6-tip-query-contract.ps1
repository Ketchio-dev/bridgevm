$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'b6-tip-query-process.ps1')
$checks = 0
function Assert-B6([bool]$Condition, [string]$Message) {
    if (!$Condition) { throw $Message }; $script:checks++
}
function Expect-B6Failure([scriptblock]$Action, [string]$Pattern) {
    $message = $null
    try { & $Action | Out-Null } catch { $message = $_.Exception.Message }
    Assert-B6 ($message -and $message -match $Pattern) "Expected failure /$Pattern/, observed [$message]"
}
. (Join-Path $PSScriptRoot 'b6-tip-query-fixtures.ps1')
$sandbox = Join-Path ([IO.Path]::GetTempPath()) ('b6-tip-headless-' + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory $sandbox | Out-Null
$runs = [Collections.Generic.List[object]]::new()
function New-TestRun([switch]$Real) {
    $value = New-B6TipEvidenceRun -EvidenceRoot (Join-Path $sandbox 'published'); $value.TestFakeOnly = !$Real
    $runs.Add($value); return $value
}
try {
    $firstState = New-B6FakeState; $firstState.ExitAt = 20; $firstState.Code = 17
    $secondState = New-B6FakeState; $secondState.ExitAt = 30; $secondState.Code = 29
    $firstFactory = New-B6FakeFactory $firstState; $secondFactory = New-B6FakeFactory $secondState
    $FixtureState = @{ Disposed = 0 }; $state = @{ Exited = $true; Code = -1 }
    $operations = @{ Elapsed = { -1 } }; $dispose = { throw 'caller disposal sentinel' }
    $first = & $firstFactory 'first' (Join-Path $sandbox 'first.out') (Join-Path $sandbox 'first.err')
    $second = & $secondFactory 'second' (Join-Path $sandbox 'second.out') (Join-Path $sandbox 'second.err')
    Assert-B6 ($firstState.Arguments -eq 'first' -and $secondState.Arguments -eq 'second') 'Factories lost their original state'
    $firstState.Code = 19; & $first.Operations.Pause
    Assert-B6 ((& $first.Operations.Elapsed) -eq 10 -and (& $second.Operations.Elapsed) -eq 0) 'Clock mutation crossed adapters'
    $firstObservation = & $first.Operations.Observe; $secondObservation = & $second.Operations.Observe
    Assert-B6 (!$firstObservation.Exited -and $firstObservation.Code -eq 19 -and !$secondObservation.Exited -and $secondObservation.Code -eq 29) 'Observation lost live fixture identity'
    & $second.Operations.Kill
    Assert-B6 ($firstState.Kills -eq 0 -and $secondState.Kills -eq 1 -and !$firstState.ForcedExit -and $secondState.ForcedExit) 'Kill mutation crossed adapters'
    $firstWait = & $first.Operations.WaitExit 5000; $secondWait = & $second.Operations.WaitExit 123
    Assert-B6 (!$firstWait -and $secondWait -and $firstState.WaitMilliseconds -eq 5000 -and $secondState.WaitMilliseconds -eq 123) 'Wait mutation crossed adapters'
    & $first.Dispose
    Assert-B6 ($firstState.Disposed -eq 1 -and $secondState.Disposed -eq 0) 'First disposal lost fixture identity'
    & $second.Dispose
    Assert-B6 ($firstState.Disposed -eq 1 -and $secondState.Disposed -eq 1 -and $FixtureState.Disposed -eq 0) 'Second disposal touched another fixture or caller'
    foreach ($code in @(0,7)) {
        $run = New-TestRun; $state = New-B6FakeState; $state.Code = $code
        $result = Invoke-B6TipQuery $run 'fixture.ps1' 42 'visible-owned' -Factory (New-B6FakeFactory $state)
        Assert-B6 ($result.Code -eq $code -and $result.Output -eq 'fixture-out' -and $result.Error -eq 'fixture-err') 'Lost normal child result'
        Assert-B6 ($state.Kills -eq 0 -and $state.Disposed -eq 1 -and $run.RawCleanupSafe) 'Normal exit ownership/disposal mismatch'
        Complete-B6TipEvidenceRun $run $true
        Assert-B6 ((Test-Path (Join-Path $run.PublishRoot '1-visible-owned.stdout.txt')) -and !(Test-Path $run.RawRoot)) 'Later assertion failure lost completed evidence'
    }
    foreach ($kind in @('killed','late-zero','kill-race','unconfirmed')) {
        $run = New-TestRun; $state = New-B6FakeState; $state.ExitAt = [double]::PositiveInfinity
        if ($kind -eq 'late-zero') { $state.ExitAt = 20 }
        if ($kind -eq 'kill-race') { $state.KillThrows = $true }
        if ($kind -eq 'unconfirmed') { $state.KillFinishes = $false }
        $pattern = 'timed out'; if ($kind -eq 'unconfirmed') { $pattern = 'cleanup unconfirmed' }
        Expect-B6Failure { Invoke-B6TipQuery $run 'fixture.ps1' 42 'visible-owned' -Factory (New-B6FakeFactory $state) } $pattern
        Assert-B6 ($state.Time -eq 20 -and $run.Cases[0].timed_out) 'Twenty-second query deadline changed'
        Assert-B6 ($state.Kills -le 1 -and $state.WaitMilliseconds -le 5000) 'Cleanup exceeded its mutation or wait bound'
        Assert-B6 ($state.Disposed -eq [int]($kind -ne 'unconfirmed')) 'Owned process disposal ignored unresolved ownership'
        if ($kind -eq 'unconfirmed') {
            Assert-B6 (!$run.RawCleanupSafe -and $run.Cases[0].streams.Count -eq 0) 'Unconfirmed child output was captured'
            Expect-B6Failure { Read-B6TipSnapshot $run $run.Cases[0] (Join-Path $run.RawRoot '1-visible-owned.stdout.raw') 'stdout' } 'before owned child exit'
            Expect-B6Failure { Invoke-B6TipQuery $run 'fixture.ps1' 42 'disabled' -Factory (New-B6FakeFactory $state) } 'Previous owned child'
        } else { Assert-B6 ($run.RawCleanupSafe -and $run.Cases[0].exit_observed) 'Exited owned child was not captured' }
        Complete-B6TipEvidenceRun $run $true
        if ($kind -eq 'unconfirmed') {
            Assert-B6 ((Test-Path $run.RawRoot) -and @([IO.Directory]::GetFiles($run.PublishRoot, '*.txt')).Count -eq 0) 'Live raw output was deleted or published'
        }
    }
    $run = New-TestRun; $state = New-B6FakeState; $state.OutBytes = [byte[]]@()
    $result = Invoke-B6TipQuery $run 'fixture.ps1' 42 'visible-owned' -Factory (New-B6FakeFactory $state)
    Assert-B6 ($result.Output -eq '' -and $run.Cases[0].streams.stdout.captured_bytes -eq 0) 'Empty stdout was not preserved'
    Complete-B6TipEvidenceRun $run $false
    foreach ($kind in @('oversized','invalid-encoding','missing','unreadable')) {
        $run = New-TestRun; $state = New-B6FakeState
        if ($kind -eq 'oversized') { $state.OutBytes = [Text.Encoding]::UTF8.GetBytes(('x' * 70000)) }
        if ($kind -eq 'invalid-encoding') { $state.OutBytes = [byte[]]@(255) }
        if ($kind -eq 'missing') { $state.MissingOut = $true }
        if ($kind -eq 'unreadable') { $state.LockOutput = $true }
        try { Expect-B6Failure { Invoke-B6TipQuery $run 'fixture.ps1' 42 'visible-owned' -Factory (New-B6FakeFactory $state) } 'capture refused' }
        finally { if ($state.OutputLock) { $state.OutputLock.Dispose() } }
        Assert-B6 ($run.Cases[0].streams.stdout.captured_bytes -le 65536) 'Unbounded stream snapshot'
        Complete-B6TipEvidenceRun $run $true
        $total = 0L; foreach ($path in [IO.Directory]::GetFiles($run.PublishRoot)) { $total += ([IO.FileInfo]$path).Length }
        Assert-B6 ($total -lt 140000) 'Single-case artifact exceeded its bound'
    }
    $run = New-TestRun; $state = New-B6FakeState; $state.AdmissionFails = $true
    Expect-B6Failure { Invoke-B6TipQuery $run 'fixture.ps1' 42 'visible-owned' -Factory (New-B6FakeFactory $state) } 'admission failed'
    Assert-B6 ($state.Kills -eq 0 -and !$run.RawCleanupSafe -and $run.Cases[0].streams.Count -eq 0) 'Unadmitted owner was used'
    Complete-B6TipEvidenceRun $run $true
    $run = New-TestRun; $state = New-B6FakeState; $state.MetadataFails = $true; $state.ExitAt = [double]::PositiveInfinity
    Expect-B6Failure { Invoke-B6TipQuery $run 'fixture.ps1' 42 'visible-owned' -Factory (New-B6FakeFactory $state) } 'metadata fixture failure'
    Assert-B6 ($state.Kills -eq 1 -and $run.RawCleanupSafe -and $run.Cases[0].exit_observed) 'Post-spawn metadata failure lost owned cleanup'
    Assert-B6 ($run.Cases[0].streams.Count -eq 2) 'Post-spawn failure lost exited child output'
    Complete-B6TipEvidenceRun $run $true
    $run = New-TestRun; $state = New-B6FakeState; $state.KillThrows = $true; $state.WaitThrows = $true
    $state.KillFinishes = $false; $state.ExitAt = [double]::PositiveInfinity
    Expect-B6Failure { Invoke-B6TipQuery $run 'fixture.ps1' 42 'visible-owned' -Factory (New-B6FakeFactory $state) } 'cleanup unconfirmed'
    Assert-B6 ($run.Cases[0].cleanup_error -match 'kill/exit race.*wait fixture failure') 'Earlier cleanup error was overwritten'
    Complete-B6TipEvidenceRun $run $true
    $run = New-TestRun
    Expect-B6Failure { Invoke-B6TipQuery $run 'fixture.ps1' 42 'visible-owned' -Factory { throw 'launch fixture failure' } } 'launch fixture failure'
    Assert-B6 (!$run.RawCleanupSafe -and $run.Cases[0].streams.Count -eq 0) 'Launch failure reused an owner'
    Complete-B6TipEvidenceRun $run $true
    $run = New-TestRun; $state = New-B6FakeState
    $null = Invoke-B6TipQuery $run 'fixture.ps1' 42 'visible-owned' -Factory (New-B6FakeFactory $state)
    [IO.File]::WriteAllBytes((Join-Path $run.Snapshots $run.Cases[0].streams.stdout.file), [byte[]]::new(70000))
    Expect-B6Failure { Complete-B6TipEvidenceRun $run $true } 'publication limit'
    Assert-B6 (!(Test-Path (Join-Path $run.PublishRoot '1-visible-owned.stdout.txt'))) 'Oversized staged file reached artifact'
    $run = New-TestRun; $state = New-B6FakeState
    Expect-B6Failure { Invoke-B6TipQuery $run 'fixture.ps1' 42 'visible-owned' -ExpectedNotFound -Factory (New-B6FakeFactory $state) } 'diagnostic-skip case'
    Assert-B6 (!$state.ContainsKey('Arguments') -and $run.Cases.Count -eq 0) 'Invalid diagnostic skip launched a query'
    $null = Invoke-B6TipQuery $run 'fixture.ps1' 42 'expected-not-found' -ExpectedNotFound -Factory (New-B6FakeFactory $state)
    Assert-B6 ($state.Arguments.EndsWith(' -ExpectedNotFound')) 'Expected absence did not reach wrapper switch'
    Complete-B6TipEvidenceRun $run $false
    $query = Join-Path $sandbox 'not-found.ps1'
    [IO.File]::WriteAllText($query, 'param($Hwnd,$Width,$Height); Write-Output "BVTIPPOINT hwnd=$Hwnd state=not-found"')
    $script:diagnosticCalls = 0
    function Add-Type { $script:diagnosticCalls++; throw 'diagnostic sentinel' }
    try {
        $global:LASTEXITCODE = 73
        $text = & (Join-Path $PSScriptRoot 'b6-tip-tree.ps1') -Query $query -Hwnd 42 -Width 1600 -Height 900 -ExpectedNotFound
        Assert-B6 ($text -eq 'BVTIPPOINT hwnd=42 state=not-found' -and $diagnosticCalls -eq 0) 'Expected absence changed query output or called UIA'
        Assert-B6 ($global:LASTEXITCODE -eq 0) 'Successful query inherited an unrelated exit status'
        Expect-B6Failure { & (Join-Path $PSScriptRoot 'b6-tip-tree.ps1') -Query $query -Hwnd 42 -Width 1600 -Height 900 } 'diagnostic sentinel'
        [IO.File]::WriteAllText($query, 'throw "query fixture failure"')
        Expect-B6Failure { & (Join-Path $PSScriptRoot 'b6-tip-tree.ps1') -Query $query -Hwnd 42 -Width 1600 -Height 900 -ExpectedNotFound } 'query fixture failure'
    } finally { Remove-Item Function:Add-Type }
    foreach ($code in @(0,7)) {
        $query = Join-Path $sandbox "child-$code.ps1"
        [IO.File]::WriteAllText($query, "[Console]::Out.WriteLine('actual-out'); [Console]::Error.WriteLine('actual-err'); exit $code")
        $run = New-TestRun -Real
        $result = Invoke-B6TipQuery $run $query 42 'visible-owned'
        Assert-B6 ($result.Code -eq $code -and $result.Output.Trim() -eq 'actual-out' -and $result.Error.Trim() -eq 'actual-err') "Actual child mismatch: expected=$code observed=$($result.Code) stdout=[$($result.Output)] stderr=[$($result.Error)]"
        Assert-B6 ($run.Cases[0].exit_observed -and !$run.Cases[0].kill_attempted) 'Actual child did not exit normally'
        Complete-B6TipEvidenceRun $run $false
        Assert-B6 (!(Test-Path $run.PublishRoot) -and !(Test-Path $run.RawRoot)) 'Passing suite retained failure output'
    }
    . (Join-Path $PSScriptRoot 'b6-tip-query-drain-contract.ps1')
    . (Join-Path $PSScriptRoot 'b6-tip-query-scope-contract.ps1')
} finally {
    $actualUnconfirmed = $false
    foreach ($run in $runs) {
        if ((Test-Path $run.RawRoot) -and !(Test-Path $run.PublishRoot) -and @($run.Cases | Where-Object { $_.failure }).Count) { Complete-B6TipEvidenceRun $run $true }
        if (!$run.TestFakeOnly -and !$run.RawCleanupSafe) { $actualUnconfirmed = $true; continue }
        if ($run.TestFakeOnly -and $global:BridgeVMB6RetainedOutputRuns) { $null = $global:BridgeVMB6RetainedOutputRuns.Remove($run) }
        if (Test-Path $run.RawRoot) { Remove-Item -Recurse -Force $run.RawRoot }
    }
    if (!$actualUnconfirmed) { Remove-Item -Recurse -Force $sandbox }
}
Write-Output "PASS: headless B6 owned-child/evidence contracts ($checks checks); no UI or guest result"
