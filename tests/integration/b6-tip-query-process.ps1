. (Join-Path $PSScriptRoot 'b6-tip-query-evidence.ps1')
. (Join-Path $PSScriptRoot 'b6-tip-query-adapter.ps1')

function Wait-B6TipOwnedChild {
    param([hashtable]$Operations, [scriptblock]$Pump = {}, [string]$InitialFailure, [ValidateRange(1,20)][double]$QueryLimitSeconds = 20)
    $expired = $false; $exited = $false; $drained = $false; $code = $null; $elapsed = 0.0
    $primary = $InitialFailure; $cleanup = [Collections.Generic.List[string]]::new(); $killed = $false
    try {
        while (!$primary) {
            $elapsed = & $Operations.Elapsed
            if ($elapsed -ge $QueryLimitSeconds) { $expired = $true; break }
            $state = & $Operations.Observe
            $elapsed = & $Operations.Elapsed
            if ($elapsed -ge $QueryLimitSeconds) { $expired = $true; break }
            $drained = $state.StreamsDrained
            if ($state.Exited) { $exited = $true; $code = $state.Code }
            if ($state.StreamError) { throw $state.StreamError }
            if ($exited -and $drained) { break }
            & $Pump
            & $Operations.Pause
        }
    } catch { $primary = $_.Exception.Message }
    if (!$exited -or !$drained) {
        $cleanupStarted = $null
        try {
            $cleanupStarted = & $Operations.Elapsed
            $state = & $Operations.Observe
            $inBudget = ((& $Operations.Elapsed) - $cleanupStarted) -lt 5; $drained = $inBudget -and $state.StreamsDrained
            if ($state.Exited) { $exited = $true; $code = $state.Code }
            elseif ($inBudget) { $killed = $true; & $Operations.Kill }
        } catch { $message = $_.Exception.Message; $cleanup.Add($message.Substring(0, [Math]::Min(240, $message.Length))) }
        if ((!$exited -or !$drained) -and $null -ne $cleanupStarted) {
            try {
                $remaining = [int][Math]::Max(0, [Math]::Floor((5 - ((& $Operations.Elapsed) - $cleanupStarted)) * 1000))
                if ($remaining -gt 0 -and (& $Operations.WaitExit $remaining) -and ((& $Operations.Elapsed) - $cleanupStarted) -lt 5) {
                    $state = & $Operations.Observe; $drained = ((& $Operations.Elapsed) - $cleanupStarted) -lt 5 -and $state.StreamsDrained
                    if ($state.Exited) { $exited = $true; $code = $state.Code }
                    if ($state.StreamError) { throw $state.StreamError }
                }
            } catch { $message = $_.Exception.Message; $cleanup.Add($message.Substring(0, [Math]::Min(240, $message.Length))) }
        }
        if ((!$exited -or !$drained) -and $null -ne $cleanupStarted -and ((& $Operations.Elapsed) - $cleanupStarted) -ge 5) { $cleanup.Add('Owned cleanup deadline expired') }
    }
    return @{ TimedOut = $expired; ExitObserved = $exited; OutputDrained = $drained; Code = $code
        Elapsed = $elapsed; PrimaryError = $primary; CleanupError = [string]::Join(' | ', $cleanup); KillAttempted = $killed }
}

function Invoke-B6TipQuery {
    param($Run, [string]$Query, [long]$Target,
        [ValidateSet('visible-owned','disabled','expected-not-found','duplicate','hidden-root')][string]$Case,
        [switch]$ExpectedNotFound, [scriptblock]$Pump = {},
        [scriptblock]$Factory = { param($arguments, $stdout, $stderr) New-B6TipOwnedProcess $arguments $stdout $stderr })
    if (!$Run.RawCleanupSafe) { throw 'Previous owned child exit remains unconfirmed' }
    if ($ExpectedNotFound -and $Case -ne 'expected-not-found') { throw 'Unexpected diagnostic-skip case' }
    $ordinal = $Run.Cases.Count + 1
    if ($ordinal -gt 5 -or @($Run.Cases | Where-Object { $_.case -eq $Case }).Count) { throw 'Invalid repeated or excess query case' }
    $prefix = "$ordinal-$Case"
    $stdout = Join-Path $Run.RawRoot "$prefix.stdout.raw"; $stderr = Join-Path $Run.RawRoot "$prefix.stderr.raw"
    $record = [ordered]@{ case = $Case; ordinal = $ordinal; query = [IO.Path]::GetFileName($Query)
        exit_observed = $false; output_drained = $false; timed_out = $false; kill_attempted = $false; streams = @{} }
    $Run.Cases.Add($record); $Run.RawCleanupSafe = $false; $adapter = $null
    Write-Host "B6 case-start: $Case"
    try {
        $diagnostic = Join-Path $PSScriptRoot 'b6-tip-tree.ps1'
        $arguments = "-NoProfile -ExecutionPolicy Bypass -File `"$diagnostic`" -Query `"$Query`" -Hwnd $Target -Width 1600 -Height 900"
        if ($ExpectedNotFound) { $arguments += ' -ExpectedNotFound' }
        $adapter = & $Factory $arguments $stdout $stderr
        if (!$adapter.Owned) { throw "Owned handle admission failed: $($adapter.AdmissionError)" }
        $record.owner = $adapter.Owner
        $wait = Wait-B6TipOwnedChild -Operations $adapter.Operations -Pump $Pump -InitialFailure $adapter.AdmissionError -QueryLimitSeconds 20
        $record.exit_observed = $wait.ExitObserved; $record.timed_out = $wait.TimedOut
        $record.kill_attempted = $wait.KillAttempted; $record.elapsed_seconds = $wait.Elapsed
        $record.cleanup_error = $wait.CleanupError; $record.primary_error = $wait.PrimaryError
        $record.output_drained = $wait.OutputDrained; if ($wait.ExitObserved) { $record.exit_code = $wait.Code }
        if ($adapter.Streams) { $record.output = @{ stdout = Get-B6TipOutputMetadata $adapter.Streams[0]; stderr = Get-B6TipOutputMetadata $adapter.Streams[1] } }
        if (!$wait.ExitObserved) { throw "Owned child cleanup unconfirmed (case=$Case)" }
        if (!$wait.OutputDrained) { throw "Owned output cleanup unconfirmed (case=$Case)" }
        $Run.RawCleanupSafe = $true
        $out = Read-B6TipSnapshot $Run $record $stdout 'stdout'
        $err = Read-B6TipSnapshot $Run $record $stderr 'stderr'
        if ($wait.TimedOut) { throw "Native UIA query timed out (case=$Case, limit=20s)" }
        if ($wait.PrimaryError) { throw $wait.PrimaryError }
        if ($out.Error -or $err.Error) { throw "Child output capture refused (case=$Case): $($out.Error) $($err.Error)" }
        return @{ Code = $wait.Code; Output = $out.Text; Error = $err.Text }
    } catch {
        $record.failure = $_.Exception.Message
        throw
    } finally {
        $disposeFailure = $null
        try {
            if ($adapter -and $adapter.Dispose) {
                if ($Run.RawCleanupSafe -or !$adapter.Owned) { & $adapter.Dispose }
                else { $Run.RetainedOwners.Add($adapter) }
            }
        }
        catch { $disposeFailure = $_.Exception.Message; $record.dispose_error = $disposeFailure; $Run.RawCleanupSafe = $false; $Run.RetainedOwners.Add($adapter) }
        Save-B6TipCaseMetadata $Run $record
        if ($disposeFailure -and !$record.failure) { throw "Owned process disposal failed: $disposeFailure" }
    }
}
