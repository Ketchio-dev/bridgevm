[CmdletBinding()]
param(
    [string]$Executable = "C:\BridgeVM\apps\ppsspp\PPSSPPWindowsARM64.exe",
    [ValidateRange(10, 600)]
    [int]$MinimumSeconds = 30,
    [string]$LogPath = "C:\BridgeVM\bvgpu-real-title-gate.log",
    # Game content to boot. Without content PPSSPP sits at its menu and
    # emits no "fps:" lines, so guest_fps reads samples=0 (measured
    # 2026-08-06); the frame counter runs only while a title renders.
    [string]$ContentPath = "",
    # The loaded module that proves the render path under test. Vulkan runs
    # require the venus ICD; D3D11 runs require the viogpu UMD instead.
    [string]$RequiredModule = "vulkan_virtio.dll",
    # Extra command-line arguments for the title (e.g. --fullscreen).
    [string]$ExtraArgs = ""
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;
public static class BridgeVMWindowActivation {
    [DllImport("user32.dll")]
    public static extern bool ShowWindowAsync(IntPtr hWnd, int nCmdShow);
    [DllImport("user32.dll")]
    public static extern bool SetForegroundWindow(IntPtr hWnd);
}
"@

function Write-GateLog {
    param([Parameter(Mandatory = $true)][string]$Message)

    $line = "[bvgpu-real-title] $Message"
    Write-Output $line
    [IO.File]::AppendAllText($LogPath, $line + [Environment]::NewLine)
}

function Convert-ToGateField {
    param(
        [AllowNull()][object]$Value,
        [ValidateRange(1, 1024)][int]$Limit = 512
    )

    if ($null -eq $Value) { return "" }
    $text = ([string]$Value) -replace '[\r\n\t]+', ' '
    if ($text.Length -gt $Limit) { return $text.Substring(0, $Limit) }
    return $text
}

function Write-ProcessExitDiagnostics {
    param(
        [Parameter(Mandatory = $true)][Diagnostics.Process]$Process,
        [Parameter(Mandatory = $true)][string]$ExecutablePath,
        [Parameter(Mandatory = $true)][DateTime]$Since,
        [Parameter(Mandatory = $true)][string]$FrameLogPath
    )

    $unsignedExit = [BitConverter]::ToUInt32(
        [BitConverter]::GetBytes([int]$Process.ExitCode), 0)
    Write-GateLog ("process_exit_ntstatus=0x{0:x8}" -f $unsignedExit)

    # Application Error event 1000 is written after the process is gone. Poll
    # only after failure, for at most five seconds, and emit fixed fields from
    # EventData rather than a locale-dependent unbounded message.
    $applicationName = Split-Path -Leaf $ExecutablePath
    $faultEvent = $null
    for ($attempt = 0; $attempt -lt 10 -and $null -eq $faultEvent; $attempt++) {
        try {
            $events = @(Get-WinEvent -FilterHashtable @{
                LogName = 'Application'
                StartTime = $Since.AddSeconds(-2)
            } -MaxEvents 64 -ErrorAction Stop)
            $faultEvent = @($events | Where-Object {
                ($_.Id -eq 1000 -or $_.ProviderName -eq 'Application Error') -and
                    $_.Message -like "*$applicationName*"
            } | Select-Object -First 1)
            if ($faultEvent.Count -eq 1) { $faultEvent = $faultEvent[0] }
            else { $faultEvent = $null }
        }
        catch { $faultEvent = $null }
        if ($null -eq $faultEvent) { Start-Sleep -Milliseconds 500 }
    }
    if ($null -ne $faultEvent) {
        try {
            [xml]$eventXml = $faultEvent.ToXml()
            $eventData = @{}
            foreach ($node in $eventXml.SelectNodes(
                "//*[local-name()='EventData']/*[local-name()='Data']")) {
                $name = $node.GetAttribute('Name')
                if ($name -ne '') { $eventData[$name] = $node.InnerText }
            }
            $fields = @()
            foreach ($name in @('AppName', 'ModuleName', 'ExceptionCode',
                    'FaultingOffset', 'AppPath', 'ModulePath')) {
                $fields += ($name.ToLowerInvariant() + '=' +
                    (Convert-ToGateField $eventData[$name] 512))
            }
            Write-GateLog ("process_exit_event provider={0} event_id={1} record_id={2} {3}" -f
                (Convert-ToGateField $faultEvent.ProviderName 64), $faultEvent.Id,
                $faultEvent.RecordId, ($fields -join ' '))
        }
        catch {
            Write-GateLog "process_exit_event status=parse-failed"
        }
    }
    else {
        Write-GateLog "process_exit_event status=not-observed-within-5s"
    }

    # Preserve only a bounded 64-KiB suffix of PPSSPP's own log in the command
    # transcript. Individual lines are capped so a corrupt log cannot inflate
    # the live receipt or violate the shared-file convention.
    if (Test-Path -LiteralPath $FrameLogPath -PathType Leaf) {
        try {
            $stream = [IO.File]::Open($FrameLogPath, [IO.FileMode]::Open,
                [IO.FileAccess]::Read, [IO.FileShare]::ReadWrite)
            try {
                $tailBudget = [Math]::Min([int64]65536, $stream.Length)
                [void]$stream.Seek(-$tailBudget, [IO.SeekOrigin]::End)
                $reader = [IO.StreamReader]::new($stream)
                $tailLines = @($reader.ReadToEnd() -split "`r?`n" |
                    Select-Object -Last 16)
                Write-GateLog "frame_tail bytes=$tailBudget lines=$($tailLines.Count)"
                foreach ($line in $tailLines) {
                    if ($line -ne '') {
                        Write-GateLog ("frame_tail_line text={0}" -f
                            (Convert-ToGateField $line 512))
                    }
                }
            }
            finally { $stream.Dispose() }
        }
        catch { Write-GateLog "frame_tail status=read-failed" }
    }
    else {
        Write-GateLog "frame_tail status=absent"
    }
}

function Get-LoadedModulePath {
    param(
        [Parameter(Mandatory = $true)][Diagnostics.Process]$Process,
        [Parameter(Mandatory = $true)][string]$ModuleName
    )

    try {
        $Process.Refresh()
        $module = @($Process.Modules | Where-Object { $_.ModuleName -ieq $ModuleName } | Select-Object -First 1)
        if ($module.Count -eq 1) {
            return $module[0].FileName
        }
    }
    catch {
        Write-GateLog "module_probe_retry error=$($_.Exception.Message)"
    }
    return $null
}

$gateMutex = [Threading.Mutex]::new($false, "Global\BridgeVMGpuRealTitleGate")
$gateMutexAcquired = $false
try {
    $gateMutexAcquired = $gateMutex.WaitOne(0)
}
catch [Threading.AbandonedMutexException] {
    $gateMutexAcquired = $true
}
if (-not $gateMutexAcquired) {
    $gateMutex.Dispose()
    exit 7
}

function Exit-Gate {
    param([Parameter(Mandatory = $true)][int]$Code)

    if ($script:gateMutexAcquired) {
        try { $script:gateMutex.ReleaseMutex() } catch { }
        $script:gateMutexAcquired = $false
    }
    $script:gateMutex.Dispose()
    exit $Code
}

[IO.File]::WriteAllText($LogPath, "")
$startedAt = [DateTime]::UtcNow
Write-GateLog "status=START utc=$($startedAt.ToString('o')) executable=$Executable minimum_seconds=$MinimumSeconds"

if (-not (Test-Path -LiteralPath $Executable -PathType Leaf)) {
    Write-GateLog "status=FAIL reason=executable-missing"
    Exit-Gate 2
}

$executableHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $Executable).Hash
Get-Process -Name "PPSSPPWindowsARM64" -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
Start-Sleep -Milliseconds 1000
$appDirectory = Split-Path -Parent $Executable
$portableSystem = Join-Path $appDirectory "memstick\PSP\SYSTEM"
$portablePoison = Join-Path $portableSystem "FailedGraphicsBackends.txt"
$canonicalConfig = Join-Path $appDirectory "bv-ppsspp.ini"
New-Item -ItemType Directory -Force -Path $portableSystem | Out-Null
Remove-Item -Force -ErrorAction SilentlyContinue -LiteralPath $portablePoison
$userSystem = "C:\Users\bridge\Documents\PPSSPP\PSP\SYSTEM"
Remove-Item -Force -ErrorAction SilentlyContinue -LiteralPath (Join-Path $userSystem "FailedGraphicsBackends.txt")
if (Test-Path -LiteralPath $canonicalConfig -PathType Leaf) {
    Copy-Item -Force -LiteralPath $canonicalConfig -Destination (Join-Path $portableSystem "ppsspp.ini")
}
Write-GateLog "launch_state_clean portable_poison_removed=$(-not (Test-Path -LiteralPath $portablePoison))"
# Frame rate has to be measured here, in the guest. The host's RESOURCE_FLUSH
# rate is not this program's frame rate: every flush in a reference run was
# issued on ctx 0 by the desktop compositor while the title rendered on its own
# context, so the host counts DWM's scanout and cannot see the app's frames.
# PPSSPP emits "delta: <ms> fps: <n>" into its own log, so let it self-report.
$frameLog = Join-Path $appDirectory "bv-frame.log"
Remove-Item -Force -ErrorAction SilentlyContinue -LiteralPath $frameLog
# PPSSPP's LogLevel enum is 1=NOTICE..6=VERBOSE. This binary never emits the
# "delta: ms fps:" line at any level (measured live at 3, 5 and 6: 7.9 MB of
# D-level log, zero fps lines, though the format string exists in the .exe).
# DEBUG is still required: the per-frame sceDisplaySetFrameBuf lines the fps
# fallback below derives samples from are D[SCEDISP].
$launchArgs = @("--log=$frameLog", "--loglevel=5")
if ($ExtraArgs -ne "") {
    $launchArgs += ($ExtraArgs -split ' ')
}
if ($ContentPath -ne "") {
    if (-not (Test-Path -LiteralPath $ContentPath -PathType Leaf)) {
        Write-GateLog "status=FAIL reason=content-missing path=$ContentPath"
        Exit-Gate 7
    }
    $launchArgs += $ContentPath
}
$process = Start-Process -FilePath $Executable -WorkingDirectory $appDirectory -PassThru -ArgumentList $launchArgs
Write-GateLog "process_started pid=$($process.Id) executable_sha256=$executableHash frame_log=$frameLog"

$deadline = $startedAt.AddSeconds($MinimumSeconds)
$venusModulePath = $null
$mainWindowObserved = $false
while ([DateTime]::UtcNow -lt $deadline) {
    Start-Sleep -Milliseconds 250
    $process.Refresh()
    if ($process.HasExited) {
        $elapsedMs = [int]([DateTime]::UtcNow - $startedAt).TotalMilliseconds
        Write-GateLog "status=FAIL reason=process-exited pid=$($process.Id) exit_code=$($process.ExitCode) elapsed_ms=$elapsedMs"
        Write-ProcessExitDiagnostics -Process $process -ExecutablePath $Executable `
            -Since $startedAt -FrameLogPath $frameLog
        Exit-Gate 3
    }
    if ($null -eq $venusModulePath) {
        $venusModulePath = Get-LoadedModulePath -Process $process -ModuleName $RequiredModule
        if ($null -ne $venusModulePath) {
            Write-GateLog "required_module_loaded module=$RequiredModule path=$venusModulePath"
        }
    }
    if (-not $mainWindowObserved -and $process.MainWindowHandle -ne [IntPtr]::Zero) {
        $window = $process.MainWindowHandle
        $restored = [BridgeVMWindowActivation]::ShowWindowAsync($window, 9)
        $foreground = [BridgeVMWindowActivation]::SetForegroundWindow($window)
        $mainWindowObserved = $true
        Write-GateLog "main_window_observed handle=$window restored=$restored foreground=$foreground"
    }
}

$process.Refresh()
if ($process.HasExited) {
    Write-GateLog "status=FAIL reason=process-exited-at-deadline pid=$($process.Id) exit_code=$($process.ExitCode)"
    Write-ProcessExitDiagnostics -Process $process -ExecutablePath $Executable `
        -Since $startedAt -FrameLogPath $frameLog
    Exit-Gate 4
}
if ($null -eq $venusModulePath) {
    Write-GateLog "status=FAIL reason=required-module-not-loaded module=$RequiredModule pid=$($process.Id)"
    Exit-Gate 5
}
if (-not $mainWindowObserved) {
    Write-GateLog "status=FAIL reason=main-window-not-observed pid=$($process.Id)"
    Exit-Gate 6
}

# Report what the title itself measured. This is observation only: no FPS
# threshold gates the result yet, because no baseline has been established on
# this stack. Absence of samples is reported as samples=0 rather than as a
# failure, so an unrelated logging change cannot silently fail the title gate.
$fpsSamples = @()
$fpsSource = "fps-lines"
$flipStamps = @()
if (Test-Path -LiteralPath $frameLog -PathType Leaf) {
    # The title still holds its log open for writing; ReadAllLines opens with
    # no write sharing and dies with a sharing violation (seen live
    # 2026-08-06). Open ReadWrite-shared instead.
    $stream = [IO.File]::Open($frameLog, [IO.FileMode]::Open,
        [IO.FileAccess]::Read, [IO.FileShare]::ReadWrite)
    try {
        $reader = New-Object IO.StreamReader($stream)
        while ($null -ne ($line = $reader.ReadLine())) {
            $m = [regex]::Match($line, 'fps:\s*([0-9]+(?:\.[0-9]+)?)')
            if ($m.Success) { $fpsSamples += [double]$m.Groups[1].Value }
            # Per-frame display flip the title logs at DEBUG: the emu core
            # presents a new framebuffer to its virtual display. Timestamp
            # prefix is M:SS:mmm (minutes without hour component).
            elseif ($line.Contains("=sceDisplaySetFrameBuf(")) {
                $ts = [regex]::Match($line, '^([0-9]+):([0-9]{2}):([0-9]{3})\s')
                if ($ts.Success) {
                    $flipStamps += ([double]$ts.Groups[1].Value * 60.0 +
                        [double]$ts.Groups[2].Value +
                        [double]$ts.Groups[3].Value / 1000.0)
                }
            }
        }
    } finally {
        $stream.Dispose()
    }
}
if ($fpsSamples.Count -eq 0 -and $flipStamps.Count -gt 1) {
    # This PPSSPP build never writes "fps:" lines (measured at loglevel 3/5/6).
    # Derive per-frame samples from the title's own sceDisplaySetFrameBuf log:
    # each interval between consecutive flips is one frame time. Hour wrap in
    # the M:SS:mmm stamps shows up as a large negative delta; skip those.
    $fpsSource = "setframebuf-intervals"
    for ($i = 1; $i -lt $flipStamps.Count; $i++) {
        $dt = $flipStamps[$i] - $flipStamps[$i - 1]
        if ($dt -gt 0.0 -and $dt -lt 5.0) { $fpsSamples += 1.0 / $dt }
    }
}
if ($fpsSamples.Count -gt 0) {
    $sorted = $fpsSamples | Sort-Object
    $p50 = $sorted[[int][Math]::Floor(($sorted.Count - 1) * 0.5)]
    $p05 = $sorted[[int][Math]::Floor(($sorted.Count - 1) * 0.05)]
    $mean = ($fpsSamples | Measure-Object -Average).Average
    Write-GateLog ("guest_fps samples={0} p50={1:F2} p05={2:F2} mean={3:F2} min={4:F2} max={5:F2} source={6}" -f `
        $fpsSamples.Count, $p50, $p05, $mean, $sorted[0], $sorted[$sorted.Count - 1], $fpsSource)
} else {
    Write-GateLog "guest_fps samples=0 reason=no-fps-lines-in-frame-log"
    # Absent samples mean the sampler found neither "fps:" lines nor the
    # per-frame sceDisplaySetFrameBuf DEBUG lines it derives them from. Report
    # bounded, fixed facts about the frame log so the cause is attributable
    # without guessing: whether it exists, its size, and how many lines of each
    # kind were seen. This is observation only and cannot turn a failure into a
    # pass.
    $frameLogBytes = -1
    if (Test-Path -LiteralPath $frameLog -PathType Leaf) {
        $frameLogBytes = (Get-Item -LiteralPath $frameLog).Length
    }
    Write-GateLog ("guest_fps_absent frame_log_bytes={0} flip_lines={1} fps_lines=0" -f
        $frameLogBytes, $flipStamps.Count)
}

$elapsedMs = [int]([DateTime]::UtcNow - $startedAt).TotalMilliseconds
Write-GateLog "status=PASS pid=$($process.Id) elapsed_ms=$elapsedMs venus_icd=$venusModulePath main_window_observed=true"
Write-GateLog "BVGPU-REAL-TITLE-PASS"
Exit-Gate 0
