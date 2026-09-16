. (Join-Path $PSScriptRoot 'b6-tip-query-streams.ps1')

function New-B6TipOwnedProcess {
    param([string]$Arguments, [string]$Stdout, [string]$Stderr)
    $child = [Diagnostics.Process]::new(); $streams = @(); $started = $false
    $clock = [Diagnostics.Stopwatch]::StartNew(); $admission = $null
    try {
        $streams += New-B6TipOutputStream $Stdout
        $streams += New-B6TipOutputStream $Stderr
        $child.StartInfo.FileName = (Get-Process -Id $PID).Path
        $child.StartInfo.Arguments = $Arguments
        $child.StartInfo.UseShellExecute = $false
        $child.StartInfo.RedirectStandardOutput = $true
        $child.StartInfo.RedirectStandardError = $true
        $started = $child.Start()
        if (!$started) { throw 'Owned child did not start' }
        $streams[0].Reader = $child.StandardOutput.BaseStream
        $streams[1].Reader = $child.StandardError.BaseStream
        foreach ($stream in $streams) { Receive-B6TipOutput $stream }
    } catch {
        if (!$started) {
            foreach ($stream in $streams) { $stream.Writer.Dispose() }
            $child.Dispose(); throw
        }
        $admission = "Owned output admission failed: $($_.Exception.Message)"
    }
    $state = @{ Process = $child; Streams = $streams; Clock = $clock }
    $operations = @{
        Elapsed = { $state.Clock.Elapsed.TotalSeconds }.GetNewClosure()
        Observe = { Get-B6TipProcessObservation $state }.GetNewClosure()
        Pause = { Start-Sleep -Milliseconds 50 }
        Kill = { $state.Process.Kill() }.GetNewClosure()
        WaitExit = { param($milliseconds) Wait-B6TipProcessCompletion $state ([int]$milliseconds) }.GetNewClosure()
    }
    $dispose = {
        $observed = Get-B6TipProcessObservation $state
        if (!$observed.Exited -or !$observed.StreamsDrained) { throw 'Cannot dispose unconfirmed child/output owner' }
        $state.Process.Dispose()
    }.GetNewClosure()
    $adapter = @{ Owned = $true; Operations = $operations; Dispose = $dispose; Owner = @{}
        Streams = $streams; AdmissionError = $admission }
    try {
        $null = $child.Handle
        $adapter.Owner.PID = $child.Id
        $adapter.Owner.Started = $child.StartTime.ToUniversalTime().ToString('o')
    } catch { $adapter.AdmissionError = "Owned child metadata failed: $($_.Exception.Message)" }
    return $adapter
}
