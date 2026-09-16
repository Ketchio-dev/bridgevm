function New-B6TipOutputStream([string]$Path) {
    return @{ Writer = [IO.File]::Open($Path, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
        Reader = $null; Pending = $null; Buffer = [byte[]]::new(4096); EOF = $false; Closed = $false
        Error = $null; SourceBytes = 0L; StoredBytes = 0L }
}

function Receive-B6TipOutput($Stream) {
    if ($Stream.Closed -or $Stream.Error -or !$Stream.Reader) { return }
    try {
        if (!$Stream.Pending) { $Stream.Pending = $Stream.Reader.ReadAsync($Stream.Buffer, 0, $Stream.Buffer.Length) }
        for ($chunk = 0; $chunk -lt 8 -and $Stream.Pending.IsCompleted; $chunk++) {
            $count = $Stream.Pending.GetAwaiter().GetResult(); $Stream.Pending = $null
            if (!$count) {
                $Stream.EOF = $true
                $Stream.Writer.Flush(); $Stream.Writer.Dispose(); $Stream.Writer = $null
                $Stream.Reader.Dispose(); $Stream.Closed = $true
                return
            }
            $Stream.SourceBytes += $count
            $keep = [int][Math]::Min($count, 65537 - $Stream.StoredBytes)
            if ($keep -gt 0) { $Stream.Writer.Write($Stream.Buffer, 0, $keep); $Stream.StoredBytes += $keep }
            # Keep draining/discarding after the acceptance cap; never grow the raw file without bound.
            $Stream.Pending = $Stream.Reader.ReadAsync($Stream.Buffer, 0, $Stream.Buffer.Length)
        }
    } catch {
        $message = $_.Exception.Message
        $Stream.Error = $message.Substring(0, [Math]::Min(240, $message.Length))
    }
}

function Get-B6TipProcessObservation($State) {
    foreach ($stream in $State.Streams) { Receive-B6TipOutput $stream }
    $exited = $State.Process.HasExited; $code = $null
    if ($exited) { $code = $State.Process.ExitCode }
    $errors = @($State.Streams | Where-Object { $_.Error } | ForEach-Object { $_.Error })
    return @{ Exited = $exited; Code = $code
        StreamsDrained = @($State.Streams | Where-Object { !$_.EOF -or !$_.Closed -or $_.Error }).Count -eq 0
        StreamError = [string]::Join(' | ', $errors) }
}

function Wait-B6TipProcessCompletion($State, [int]$Milliseconds) {
    $clock = [Diagnostics.Stopwatch]::StartNew()
    while ($clock.Elapsed.TotalMilliseconds -lt $Milliseconds) {
        $observation = Get-B6TipProcessObservation $State
        if ($observation.Exited -and $observation.StreamsDrained) { return $clock.Elapsed.TotalMilliseconds -lt $Milliseconds }
        $remaining = [int][Math]::Max(0, $Milliseconds - $clock.Elapsed.TotalMilliseconds)
        if (!$remaining) { break }
        $pending = @($State.Streams | Where-Object { $_.Pending -and !$_.Error } | ForEach-Object { $_.Pending })
        if ($pending.Count) { $null = [Threading.Tasks.Task]::WaitAny([Threading.Tasks.Task[]]$pending, $remaining) }
        elseif (!$observation.Exited) { $null = $State.Process.WaitForExit($remaining) }
        else { break }
    }
    return $false
}

function Get-B6TipOutputMetadata($Stream) {
    return @{ eof = $Stream.EOF; writer_closed = $Stream.Closed; source_bytes = $Stream.SourceBytes
        stored_bytes = $Stream.StoredBytes; error = $Stream.Error }
}

function Register-B6TipOutputRun($Run) {
    # Reserve active and unresolved work before effects; never evict an unknown owner.
    $held = Get-Variable -Name BridgeVMB6RetainedOutputRuns -Scope Global -ErrorAction SilentlyContinue
    if (!$held) { $global:BridgeVMB6RetainedOutputRuns = [Collections.Generic.List[object]]::new() }
    if ($global:BridgeVMB6RetainedOutputRuns.Contains($Run)) { return }
    if ($global:BridgeVMB6RetainedOutputRuns.Count -ge 16) { throw 'Owned output run capacity reached; unresolved owners remain retained' }
    $global:BridgeVMB6RetainedOutputRuns.Add($Run)
}

function Set-B6TipOutputRetention($Run) {
    # A caller's script scope can end while its PowerShell host remains alive.
    if ($Run.RawCleanupSafe) { $null = $global:BridgeVMB6RetainedOutputRuns.Remove($Run) }
    else { Register-B6TipOutputRun $Run }
}

function New-B6TipEvidenceRun {
    param([string]$EvidenceRoot = $env:B6_TIP_EVIDENCE_ROOT)
    $id = [Guid]::NewGuid().ToString('N')
    $raw = Join-Path ([IO.Path]::GetTempPath()) "b6-tip-$id"; $snapshots = Join-Path $raw 'bounded'
    if (!$EvidenceRoot) { $EvidenceRoot = Join-Path ([IO.Path]::GetTempPath()) 'bridgevm-b6-tip-evidence' }
    $run = @{ RawRoot = $raw; Snapshots = $snapshots; PublishRoot = (Join-Path $EvidenceRoot $id)
        RawCleanupSafe = $true; Cases = [Collections.Generic.List[object]]::new(); RetainedOwners = [Collections.Generic.List[object]]::new() }
    Register-B6TipOutputRun $run
    try { New-Item -ItemType Directory $snapshots -ErrorAction Stop | Out-Null }
    catch {
        $failure = $_; $run.InitializationError = $failure.Exception.Message
        try {
            if (Test-Path -LiteralPath $raw -ErrorAction Stop) { Remove-Item -LiteralPath $raw -Recurse -Force -ErrorAction Stop }
            Set-B6TipOutputRetention $run
        } catch { $run.RawCleanupSafe = $false; $run.InitializationCleanupError = $_.Exception.Message }
        throw $failure
    }
    return $run
}
