. (Join-Path $PSScriptRoot 'b6-tip-query-process.ps1')

function Test-BvInventoryErrorResult($Code, [string]$Stdout, [string]$Stderr) {
    return $null -ne $Code -and $Code -ne 0 -and $Stdout.Length -eq 0 -and $Stderr.Length -gt 0
}

function Invoke-BvInventoryErrorChild {
    param($Run, [ValidateSet('invalid_utf8','native_failure','missing_provider')][string]$Name,
        [string]$Script, [scriptblock]$Factory = {
            param($arguments, $stdout, $stderr) New-B6TipOwnedProcess $arguments $stdout $stderr
        })
    if (!$Run.RawCleanupSafe) { throw 'Previous inventory child/output owner remains unconfirmed' }
    $ordinal = $Run.Cases.Count + 1
    if ($ordinal -gt 3) { throw 'Too many inventory error cases' }
    $stdout = Join-Path $Run.RawRoot "$ordinal-$Name.stdout.raw"
    $stderr = Join-Path $Run.RawRoot "$ordinal-$Name.stderr.raw"
    $record = [ordered]@{ case = $Name; ordinal = $ordinal; query = [IO.Path]::GetFileName($Script)
        exit_observed = $false; output_drained = $false; timed_out = $false; streams = @{} }
    $Run.Cases.Add($record); $Run.RawCleanupSafe = $false; $adapter = $null
    try {
        $arguments = '-NoLogo -NoProfile -File "' + $Script + '"'
        $adapter = & $Factory $arguments $stdout $stderr
        if (!$adapter.Owned) { throw "Inventory child ownership not admitted: $($adapter.AdmissionError)" }
        $record.owner = $adapter.Owner
        $wait = Wait-B6TipOwnedChild -Operations $adapter.Operations -InitialFailure $adapter.AdmissionError -QueryLimitSeconds 10
        $record.exit_observed = $wait.ExitObserved; $record.output_drained = $wait.OutputDrained
        $record.timed_out = $wait.TimedOut; $record.elapsed_seconds = $wait.Elapsed
        $record.kill_attempted = $wait.KillAttempted; $record.exit_code = $wait.Code
        $record.primary_error = $wait.PrimaryError; $record.cleanup_error = $wait.CleanupError
        if ($adapter.Streams) {
            $record.output = @{ stdout = Get-B6TipOutputMetadata $adapter.Streams[0]
                stderr = Get-B6TipOutputMetadata $adapter.Streams[1] }
        }
        $prefix = if ($wait.TimedOut) { "$Name child deadline; " } else { '' }
        if (!$wait.ExitObserved) { throw ($prefix + 'inventory child exit unconfirmed') }
        if (!$wait.OutputDrained) { throw ($prefix + 'inventory output drain unconfirmed') }
        $Run.RawCleanupSafe = $true
        $out = Read-B6TipSnapshot $Run $record $stdout 'stdout'
        $err = Read-B6TipSnapshot $Run $record $stderr 'stderr'
        if ($wait.TimedOut) { throw "$Name child deadline" }
        if ($wait.PrimaryError) { throw $wait.PrimaryError }
        if ($wait.CleanupError) { throw $wait.CleanupError }
        if ($out.Error -or $err.Error) { throw "Inventory output capture refused: $($out.Error) $($err.Error)" }
        if (!(Test-BvInventoryErrorResult $wait.Code $out.Text $err.Text)) {
            throw ($Name + ' did not fail closed under unguarded Continue')
        }
        return @{ Code = $wait.Code; Output = $out.Text; Error = $err.Text }
    } catch {
        $record.failure = $_.Exception.Message
        throw
    } finally {
        $disposeFailure = $null
        if ($adapter) {
            if ($Run.RawCleanupSafe) {
                try { & $adapter.Dispose }
                catch {
                    $disposeFailure = $_.Exception.Message; $record.dispose_error = $disposeFailure
                    $Run.RawCleanupSafe = $false; $Run.RetainedOwners.Add($adapter)
                }
            } else { $Run.RetainedOwners.Add($adapter) }
        }
        try { Save-B6TipCaseMetadata $Run $record }
        catch {
            if (!$record.failure -and !$disposeFailure) { throw }
            Write-Host 'Inventory case metadata could not be saved; original failure retained'
        }
        if ($disposeFailure -and !$record.failure) { throw "Inventory owner disposal failed: $disposeFailure" }
    }
}

function Complete-BvInventoryErrorRun($Run, [string]$FixtureRoot, [bool]$Failed) {
    if (!$Run.RawCleanupSafe) {
        $Run.FixtureRoot = $FixtureRoot
        Write-Host "Inventory cleanup unconfirmed; fixture and retained owners preserved at $FixtureRoot"
    }
    try { Complete-B6TipEvidenceRun $Run $Failed }
    catch {
        if (!$Failed) { throw }
        Write-Host 'Inventory evidence publication failed; original test failure retained'
    } finally {
        if ($Run.RawCleanupSafe) {
            try { Remove-Item -LiteralPath $FixtureRoot -Recurse -Force }
            catch { if (!$Failed) { throw }; Write-Host 'Inventory fixture removal failed; original test failure retained' }
        }
    }
}
