[CmdletBinding()]
param(
    [Parameter(Mandatory=$true)][ValidatePattern('^[0-9a-f]{32}$')][string]$Nonce,
    [Parameter(Mandatory=$true)][ValidatePattern('^[0-9a-f]{64}$')][string]$ExpectedD3D11UmdSha
)

# One real, visible VLC/AV1 playback. This script reports observations only;
# the host independently checks the bound CSV and changing GPU scanout.
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$share = 'C:\BridgeVMB9'
$work = Join-Path 'C:\BridgeVM' ('b9-work-' + $Nonce)
$readyPath = Join-Path $share ('ready-' + $Nonce + '.json')
$collectorPath = Join-Path $share ('collector-' + $Nonce + '.json')
$finishedPath = Join-Path $share ('finished-' + $Nonce + '.json')
$mediaPath = Join-Path $share 'bbb_1080p_10s_5MB_av1.webm'
$presentMonPath = Join-Path $share 'PresentMon-2.5.1-x64.exe'
$csvPath = Join-Path $share ('b9-' + $Nonce + '.csv')
$utf8 = New-Object Text.UTF8Encoding($false)
$zipHash = '9c0917dc521ffc8ce30e70bca7f6c9dc8fec80909d763e75cd976351dee8db0b'
$mediaHash = '5e43740e2916afc1b17de09f4948f038b83065a5d42f0fcb16643d7faeb00ad7'
$presentMonHash = '9bec3083069f58f911e6a512f4806db51a27bd096103087bc1d05ef54c80a191'
$state = [ordered]@{
    schema = 'bridgevm.b9-vlc-finished.v1'; nonce = $Nonce; pid = 0; hwnd = 0
    window_visible = $false
    vlc_exit_observed = $false; vlc_exit_code = -1; playback_elapsed_ms = 0
    collector_started = $false; collector_exit_code = -1
    csv_sha256 = ''; csv_bytes = 0; direct3d11_module_loaded = $false
    av1_decoder_module_loaded = $false; driver_umd_sha256 = ''
    failure_code = 'PREPARATION_FAILED'; failure_detail = ''
}
$vlc = $null
$collector = $null
$mutex = New-Object Threading.Mutex($false, 'Global\BridgeVMB9VlcPlayback')
$owned = $false
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class BridgeVMB9Window {
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hwnd);
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hwnd, out uint pid);
}
'@

function Write-Observation([string]$Path, [object]$Value) {
    if (Test-Path -LiteralPath $Path) { throw 'B9 observation already exists' }
    $raw = (ConvertTo-Json -InputObject $Value -Depth 4 -Compress) + "`r`n"
    if ($raw.Length -gt 8192) { throw 'B9 observation exceeds private bound' }
    [IO.File]::WriteAllText($Path, $raw, $utf8)
}

function Assert-Hash([string]$Path, [string]$Expected) {
    $item = Get-Item -LiteralPath $Path -ErrorAction Stop
    if ($item.PSIsContainer -or ($item.Attributes -band [IO.FileAttributes]::ReparsePoint)) {
        throw 'B9 source must be a regular file'
    }
    if ((Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant() -cne $Expected) {
        throw 'B9 source hash mismatch'
    }
}

function Wait-HostMarker([string]$Name, [int]$Seconds) {
    $path = Join-Path $share $Name
    $deadline = [DateTime]::UtcNow.AddSeconds($Seconds)
    while ([DateTime]::UtcNow -lt $deadline) {
        if (Test-Path -LiteralPath $path -PathType Leaf) {
            $raw = [IO.File]::ReadAllText($path, $utf8).Trim()
            if ($raw -cne $Nonce) { throw 'B9 host marker nonce differs' }
            return
        }
        if ($null -ne $vlc) { $vlc.Refresh(); if ($vlc.HasExited) { throw 'VLC exited before playback was released' } }
        Start-Sleep -Milliseconds 200
    }
    throw 'B9 host marker timeout'
}

try {
    try { $owned = $mutex.WaitOne(0) }
    catch [Threading.AbandonedMutexException] { $owned = $true }
    if (-not $owned) { throw 'another B9 playback owns the guest mutex' }
    foreach ($path in @($readyPath, $collectorPath, $finishedPath, $csvPath, $work)) {
        if (Test-Path -LiteralPath $path) { throw 'B9 run artifact already exists' }
    }
    Assert-Hash $mediaPath $mediaHash
    Assert-Hash $presentMonPath $presentMonHash
    $partsPath = Join-Path $share 'b9-vlc-parts.tsv'
    $parts = @([IO.File]::ReadAllLines($partsPath, $utf8))
    if ($parts.Count -ne 10) { throw 'B9 VLC chunk count differs' }
    New-Item -ItemType Directory -Path $work -ErrorAction Stop | Out-Null
    $archive = Join-Path $work 'vlc.zip'
    $output = [IO.File]::Open($archive, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
    try {
        for ($i = 0; $i -lt $parts.Count; $i++) {
            $columns = $parts[$i].Split("`t")
            $name = ('b9-vlc-part-{0:D2}.bin' -f $i)
            if ($columns.Count -ne 3 -or $columns[0] -cne $name -or
                $columns[1] -cnotmatch '^[0-9]{1,7}$' -or
                $columns[2] -cnotmatch '^[0-9a-f]{64}$') { throw 'invalid VLC chunk manifest' }
            $partPath = Join-Path $share $name
            $part = Get-Item -LiteralPath $partPath -ErrorAction Stop
            if ($part.Length -ne [int]$columns[1] -or $part.Length -gt 7500000) {
                throw 'VLC chunk size differs'
            }
            Assert-Hash $partPath $columns[2]
            $input = [IO.File]::OpenRead($partPath)
            try { $input.CopyTo($output) } finally { $input.Dispose() }
        }
    } finally { $output.Dispose() }
    Assert-Hash $archive $zipHash
    Expand-Archive -LiteralPath $archive -DestinationPath $work -ErrorAction Stop
    $exe = Join-Path $work 'vlc-3.0.23\vlc.exe'
    if (-not (Test-Path -LiteralPath $exe -PathType Leaf)) { throw 'VLC executable absent from pinned archive' }
    $command = '"' + $exe + '" --no-one-instance --play-and-exit --start-paused ' +
        '--vout=direct3d11 --no-video-title-show --no-qt-privacy-ask "' + $mediaPath + '"'
    $created = Invoke-CimMethod -ClassName Win32_Process -MethodName Create -Arguments @{ CommandLine = $command }
    if ($created.ReturnValue -ne 0 -or $created.ProcessId -le 0) { throw 'VLC CIM launch failed' }
    $state.pid = [int]$created.ProcessId
    $vlc = [Diagnostics.Process]::GetProcessById($state.pid)
    $identity = Get-CimInstance -ClassName Win32_Process -Filter ("ProcessId=" + $state.pid)
    if ($vlc.ProcessName -ine 'vlc' -or $null -eq $identity -or
        $identity.Name -ine 'vlc.exe' -or $identity.ExecutablePath -ine $exe) {
        throw 'launched process does not have the pinned VLC filename and path'
    }
    $deadline = [DateTime]::UtcNow.AddSeconds(90)
    $hwnd = [IntPtr]::Zero
    while ([DateTime]::UtcNow -lt $deadline) {
        $vlc.Refresh()
        if ($vlc.HasExited) { throw 'VLC exited before a visible window appeared' }
        $hwnd = $vlc.MainWindowHandle
        if ($hwnd -ne [IntPtr]::Zero -and $vlc.MainWindowTitle.Length -gt 0 -and
            [BridgeVMB9Window]::IsWindowVisible($hwnd)) { break }
        Start-Sleep -Milliseconds 250
    }
    if ($hwnd -eq [IntPtr]::Zero) { throw 'VLC visible window deadline exceeded' }
    [uint32]$windowPid = 0
    [void][BridgeVMB9Window]::GetWindowThreadProcessId($hwnd, [ref]$windowPid)
    if ($windowPid -ne [uint32]$state.pid) { throw 'VLC visible window is not owned by launched PID' }
    $state.hwnd = $hwnd.ToInt64()
    $state.window_visible = $true
    Write-Observation $readyPath ([ordered]@{
        schema = 'bridgevm.b9-vlc-ready.v1'; nonce = $Nonce; pid = $state.pid
        hwnd = $state.hwnd; title = $vlc.MainWindowTitle; media_sha256 = $mediaHash
        vlc_zip_sha256 = $zipHash; presentmon_sha256 = $presentMonHash
        expected_driver_umd_sha256 = $ExpectedD3D11UmdSha
    })
    $state.failure_code = 'COLLECTOR_FAILED'
    Wait-HostMarker ('collector-go-' + $Nonce + '.txt') 60
    $session = 'BridgeVM-B9-' + $Nonce
    $arguments = '--process_id ' + $state.pid + ' --timed 25 --terminate_after_timed ' +
        '--v2_metrics --session_name "' + $session + '" --output_file "' + $csvPath + '"'
    $collector = Start-Process -FilePath $presentMonPath -ArgumentList $arguments -PassThru -WindowStyle Hidden
    Start-Sleep -Seconds 2
    $collector.Refresh()
    if ($collector.HasExited) { throw 'PresentMon exited before capture handshake' }
    $state.collector_started = $true
    Write-Observation $collectorPath ([ordered]@{
        schema = 'bridgevm.b9-collector-ready.v1'; nonce = $Nonce; pid = $state.pid
        session = $session; collector_pid = $collector.Id
    })
    $state.failure_code = 'PLAYBACK_INCOMPLETE'
    Wait-HostMarker ('play-start-' + $Nonce + '.txt') 60
    $playStart = [DateTime]::UtcNow
    $playDeadline = $playStart.AddSeconds(60)
    while ([DateTime]::UtcNow -lt $playDeadline) {
        if ($vlc.WaitForExit(250)) { break }
        $named = @(Get-Process -Name vlc -ErrorAction SilentlyContinue | Where-Object { $_.Id -eq $state.pid })
        if ($named.Count -ne 1) { throw 'VLC filename/PID disappeared before completion' }
        try {
            if (@($vlc.Modules | Where-Object { $_.ModuleName -ieq 'libdirect3d11_plugin.dll' }).Count -eq 1) {
                $state.direct3d11_module_loaded = $true
            }
            if (@($vlc.Modules | Where-Object {
                $_.ModuleName -ieq 'libdav1d_plugin.dll' -or $_.ModuleName -ieq 'libaom_plugin.dll'
            }).Count -ge 1) { $state.av1_decoder_module_loaded = $true }
            $umd = @($vlc.Modules | Where-Object { $_.ModuleName -ieq 'viogpu_d3d10.dll' })
            if ($umd.Count -eq 1 -and $state.driver_umd_sha256 -eq '') {
                $state.driver_umd_sha256 = (Get-FileHash -LiteralPath $umd[0].FileName -Algorithm SHA256).Hash.ToLowerInvariant()
            }
        } catch { }
    }
    $vlc.Refresh()
    if (-not $vlc.HasExited) { throw 'VLC did not exit after the bounded playback window' }
    $state.vlc_exit_observed = $true
    $state.vlc_exit_code = $vlc.ExitCode
    $state.playback_elapsed_ms = [int]([DateTime]::UtcNow - $playStart).TotalMilliseconds
    if (-not $collector.WaitForExit(40000)) { throw 'PresentMon did not exit after its timed capture' }
    $state.collector_exit_code = $collector.ExitCode
    if ($collector.ExitCode -ne 0) { throw 'PresentMon exited nonzero' }
    $csv = Get-Item -LiteralPath $csvPath -ErrorAction Stop
    if ($csv.PSIsContainer -or $csv.Length -le 0 -or $csv.Length -gt 7500000) {
        throw 'PresentMon CSV is missing or exceeds share limit'
    }
    $state.csv_bytes = [int]$csv.Length
    $state.csv_sha256 = (Get-FileHash -LiteralPath $csvPath -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($state.vlc_exit_code -ne 0 -or $state.playback_elapsed_ms -lt 8000 -or
        -not $state.direct3d11_module_loaded -or -not $state.av1_decoder_module_loaded -or
        $state.driver_umd_sha256 -cne $ExpectedD3D11UmdSha) {
        throw 'VLC exit or playback duration differs'
    }
    $state.failure_code = 'none'
} catch {
    $state.failure_code = [string]$state.failure_code
    $state.failure_detail = ([string]$_.Exception.Message).Substring(0,
        [Math]::Min(160, ([string]$_.Exception.Message).Length))
} finally {
    if ($null -ne $collector) {
        $collector.Refresh()
        if (-not $collector.HasExited) { $collector.Kill(); [void]$collector.WaitForExit(5000) }
        $collector.Dispose()
    }
    if ($null -ne $vlc) {
        $vlc.Refresh()
        if (-not $vlc.HasExited) { $vlc.Kill(); [void]$vlc.WaitForExit(5000) }
        $vlc.Dispose()
    }
    try { Write-Observation $finishedPath $state } catch { }
    if ($owned) { $mutex.ReleaseMutex() }
    $mutex.Dispose()
}
if ($state.failure_code -ne 'none') { exit 1 }
