# Real adapters must outlive their defining scope without ambient helper lookup.
$scopeFixtures = [Collections.Generic.List[object]]::new()
$savedHelpers = @{}
foreach ($name in @('Receive-B6TipOutput','Get-B6TipProcessObservation','Wait-B6TipProcessCompletion')) {
    $savedHelpers[$name] = (Get-Item "Function:$name").ScriptBlock
}
try {
    foreach ($code in @(0,7)) {
        $run = New-TestRun -Real; $run.RawCleanupSafe = $false
        $query = Join-Path $sandbox "scope child $code.ps1"
        [IO.File]::WriteAllText($query, "[Console]::Out.Write('scope-out-$code'); [Console]::Error.Write('scope-err-$code'); exit $code")
        $adapter = & {
            param($source, $queryPath, $rawRoot)
            . $source
            New-B6TipOwnedProcess "-NoProfile -File `"$queryPath`"" (Join-Path $rawRoot 'out.raw') (Join-Path $rawRoot 'err.raw')
        } (Join-Path $PSScriptRoot 'b6-tip-query-adapter.ps1') $query $run.RawRoot
        $run.RetainedOwners.Add($adapter)
        $scopeFixtures.Add(@{ Run = $run; Adapter = $adapter; Code = $code; Disposed = $false })
    }
    foreach ($name in $savedHelpers.Keys) { Set-Item "Function:$name" -Value { throw 'Ambient helper lookup escaped retained adapter scope' } }
    $state = @{ Process = 'unrelated caller state' }
    foreach ($fixture in $scopeFixtures) {
        $adapter = $fixture.Adapter
        $null = & $adapter.Operations.Observe
        Assert-B6 (& $adapter.Operations.WaitExit 5000) 'Retained helper graph did not finish actual child output'
        $observation = & $adapter.Operations.Observe
        Assert-B6 ($observation.Exited -and $observation.Code -eq $fixture.Code -and $observation.StreamsDrained) 'Scope-exited adapter lost its own child exit or stream proof'
        & $adapter.Dispose; $fixture.Disposed = $true; $fixture.Run.RawCleanupSafe = $true
        foreach ($stream in @('out','err')) {
            $text = [IO.File]::ReadAllText((Join-Path $fixture.Run.RawRoot "$stream.raw"))
            Assert-B6 ($text -eq "scope-$stream-$($fixture.Code)") 'Retained helper scope crossed adapter output or lost exact bytes'
        }
    }
} finally {
    foreach ($name in $savedHelpers.Keys) { Set-Item "Function:$name" -Value $savedHelpers[$name] }
    foreach ($fixture in $scopeFixtures) {
        if (!$fixture.Disposed) {
            try {
                if (& $fixture.Adapter.Operations.WaitExit 5000) {
                    & $fixture.Adapter.Dispose; $fixture.Disposed = $true; $fixture.Run.RawCleanupSafe = $true
                }
            } catch { Write-Host "Scope fixture cleanup remains unconfirmed: $($_.Exception.Message)" }
        }
        Complete-B6TipEvidenceRun $fixture.Run $false
    }
}
