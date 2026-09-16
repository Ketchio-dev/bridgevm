function New-B6FakeState {
    return @{ Time = 0.0; ExitAt = 0.0; Code = 0; ForcedExit = $false; Kills = 0
        KillFinishes = $true; KillThrows = $false; WaitThrows = $false; WaitMilliseconds = 0; Disposed = 0
        OutBytes = [Text.Encoding]::UTF8.GetBytes('fixture-out'); MissingOut = $false; AdmissionFails = $false; MetadataFails = $false; LockOutput = $false }
}

function New-B6FakeFactory([hashtable]$FixtureState) {
    # Bind each operation while FixtureState is a direct local parameter.
    $operations = @{
        Elapsed = { $FixtureState.Time }.GetNewClosure()
        Observe = { @{ Exited = ($FixtureState.ForcedExit -or $FixtureState.Time -ge $FixtureState.ExitAt); Code = $FixtureState.Code } }.GetNewClosure()
        Pause = { $FixtureState.Time += 10 }.GetNewClosure()
        Kill = { $FixtureState.Kills++; if ($FixtureState.KillFinishes) { $FixtureState.ForcedExit = $true }; if ($FixtureState.KillThrows) { throw 'kill/exit race' } }.GetNewClosure()
        WaitExit = { param($milliseconds) $FixtureState.WaitMilliseconds = $milliseconds; if ($FixtureState.WaitThrows) { throw 'wait fixture failure' }; $FixtureState.ForcedExit -or $FixtureState.Time -ge $FixtureState.ExitAt }.GetNewClosure()
    }
    $dispose = { $FixtureState.Disposed++ }.GetNewClosure()
    return {
        param($arguments, $stdout, $stderr)
        $FixtureState.Arguments = $arguments
        if (!$FixtureState.MissingOut) { [IO.File]::WriteAllBytes($stdout, $FixtureState.OutBytes) }
        [IO.File]::WriteAllText($stderr, 'fixture-err', [Text.UTF8Encoding]::new($false))
        if ($FixtureState.LockOutput) { $FixtureState.OutputLock = [IO.File]::Open($stdout, [IO.FileMode]::Open, [IO.FileAccess]::ReadWrite, [IO.FileShare]::None) }
        $admission = $null
        if ($FixtureState.AdmissionFails) { $admission = 'fixture admission failure' }
        if ($FixtureState.MetadataFails) { $admission = 'owned child metadata fixture failure' }
        return @{ Owned = !$FixtureState.AdmissionFails; AdmissionError = $admission; Owner = @{ PID = 17; Started = 'fixture' }
            Operations = $operations; Dispose = $dispose }
    }.GetNewClosure()
}
