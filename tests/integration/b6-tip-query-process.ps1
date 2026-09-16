. (Join-Path $PSScriptRoot 'b6-tip-query-evidence.ps1')

function Wait-B6TipOwnedChild {
    param([hashtable]$Operations, [scriptblock]$Pump = {}, [string]$InitialFailure)
    $expired = $false; $exited = $false; $code = $null; $elapsed = 0.0
    $primary = $InitialFailure; $cleanup = [Collections.Generic.List[string]]::new(); $killed = $false
    try {
        while (!$primary) {
            $elapsed = & $Operations.Elapsed
            if ($elapsed -ge 20) { $expired = $true; break }
            $state = & $Operations.Observe
            $elapsed = & $Operations.Elapsed
            if ($elapsed -ge 20) { $expired = $true; break }
            if ($state.Exited) { $exited = $true; $code = $state.Code; break }
            & $Pump
            & $Operations.Pause
        }
    } catch { $primary = $_.Exception.Message }
    if (!$exited) {
        try {
            $state = & $Operations.Observe
            if ($state.Exited) { $exited = $true; $code = $state.Code }
            else { $killed = $true; & $Operations.Kill }
        } catch { $message = $_.Exception.Message; $cleanup.Add($message.Substring(0, [Math]::Min(240, $message.Length))) }
        if (!$exited) {
            try {
                if (& $Operations.WaitExit 5000) {
                    $state = & $Operations.Observe
                    if ($state.Exited) { $exited = $true; $code = $state.Code }
                }
            } catch { $message = $_.Exception.Message; $cleanup.Add($message.Substring(0, [Math]::Min(240, $message.Length))) }
        }
    }
    return @{ TimedOut = $expired; ExitObserved = $exited; Code = $code
        Elapsed = $elapsed; PrimaryError = $primary; CleanupError = [string]::Join(' | ', $cleanup); KillAttempted = $killed }
}

function New-B6TipOwnedProcess {
    param([string]$Arguments, [string]$Stdout, [string]$Stderr)
    $child = Start-Process -FilePath (Get-Process -Id $PID).Path -ArgumentList $Arguments -PassThru -RedirectStandardOutput $Stdout -RedirectStandardError $Stderr
    $dispose = { $child.Dispose() }.GetNewClosure()
    $clock = [Diagnostics.Stopwatch]::StartNew()
    $operations = @{
        Elapsed = { $clock.Elapsed.TotalSeconds }.GetNewClosure()
        Observe = { $done = $child.HasExited; $value = $null; if ($done) { $value = $child.ExitCode }; @{ Exited = $done; Code = $value } }.GetNewClosure()
        Pause = { Start-Sleep -Milliseconds 50 }
        Kill = { $child.Kill() }.GetNewClosure()
        WaitExit = { param($milliseconds) $child.WaitForExit([int]$milliseconds) }.GetNewClosure()
    }
    $adapter = @{ Owned = $true; Operations = $operations; Dispose = $dispose; Owner = @{} }
    try {
        $null = $child.Handle
        $adapter.Owner.PID = $child.Id
        $adapter.Owner.Started = $child.StartTime.ToUniversalTime().ToString('o')
    } catch { $adapter.AdmissionError = "Owned child metadata failed: $($_.Exception.Message)" }
    return $adapter
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
        exit_observed = $false; timed_out = $false; kill_attempted = $false; streams = @{} }
    $Run.Cases.Add($record); $Run.RawCleanupSafe = $false; $adapter = $null
    Write-Host "B6 case-start: $Case"
    try {
        $diagnostic = Join-Path $PSScriptRoot 'b6-tip-tree.ps1'
        $arguments = "-NoProfile -ExecutionPolicy Bypass -File `"$diagnostic`" -Query `"$Query`" -Hwnd $Target -Width 1600 -Height 900"
        if ($ExpectedNotFound) { $arguments += ' -ExpectedNotFound' }
        $adapter = & $Factory $arguments $stdout $stderr
        if (!$adapter.Owned) { throw "Owned handle admission failed: $($adapter.AdmissionError)" }
        $record.owner = $adapter.Owner
        $wait = Wait-B6TipOwnedChild -Operations $adapter.Operations -Pump $Pump -InitialFailure $adapter.AdmissionError
        $record.exit_observed = $wait.ExitObserved; $record.timed_out = $wait.TimedOut
        $record.kill_attempted = $wait.KillAttempted; $record.elapsed_seconds = $wait.Elapsed
        $record.cleanup_error = $wait.CleanupError; $record.primary_error = $wait.PrimaryError
        if (!$wait.ExitObserved) { throw "Owned child cleanup unconfirmed (case=$Case)" }
        $Run.RawCleanupSafe = $true; $record.exit_code = $wait.Code
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
        try { if ($adapter -and $adapter.Dispose) { & $adapter.Dispose } }
        catch { $disposeFailure = $_.Exception.Message; $record.dispose_error = $disposeFailure }
        Save-B6TipCaseMetadata $Run $record
        if ($disposeFailure -and !$record.failure) { throw "Owned process disposal failed: $disposeFailure" }
    }
}
