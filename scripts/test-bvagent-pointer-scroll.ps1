$ErrorActionPreference = 'Stop'
& (Join-Path $PSScriptRoot 'test-bvagent-pointer-input.ps1')
foreach ($ticks in -128..127) {
    if ($ticks -eq 0) { continue }
    foreach ($point in @('0x32767', '32767x0')) {
        $events = [BridgeVM.BvUnicodeInput]::BuildPointer(('scroll:{0}@{1}' -f $ticks, $point))
        if ($events.Length -ne 2 -or $events[0].Type -ne 0 -or $events[1].Type -ne 0) {
            throw 'scroll must contain exactly one move followed by one wheel'
        }
        $expectedX = if ($point -eq '0x32767') { 0 } else { 65535 }
        $expectedY = 65535 - $expectedX
        if ($events[0].Data.Mouse.X -ne $expectedX -or $events[0].Data.Mouse.Y -ne $expectedY -or
            $events[0].Data.Mouse.Flags -ne 0x8001 -or $events[1].Data.Mouse.Flags -ne 0x0800) {
            throw 'scroll coordinate or event order mismatch'
        }
        $signed = [BitConverter]::ToInt32([BitConverter]::GetBytes([uint32]$events[1].Data.Mouse.MouseData), 0)
        if ($signed -ne ($ticks * 120)) { throw 'scroll tick magnitude or sign lost' }
    }
}
foreach ($command in @('scroll:0@0x0', 'scroll:-129@0x0', 'scroll:128@0x0', 'scroll:1',
    'scroll:1@', 'scroll:1@0x0@0x0', 'scroll:+1@0x0', 'scroll:01@0x0', 'scroll:-0@0x0',
    'scroll:1@32768x0', 'scroll:1@0x-1', 'scroll:1@00x0', "scroll:1@0x0`n", 'wheel:1@0x0')) {
    $rejected = $false
    try { $null = [BridgeVM.BvUnicodeInput]::BuildPointer($command) } catch { $rejected = $true }
    if (-not $rejected) { throw ('malformed scroll accepted: ' + $command) }
}
function Signed($event) { [BitConverter]::ToInt32([BitConverter]::GetBytes([uint32]$event.Data.Mouse.MouseData), 0) }
# scrollunits:<v>,<h>@XxY carries raw wheel units: a move, a WHEEL (0x0800) when v != 0, an HWHEEL (0x1000) when h != 0.
foreach ($case in @(@('scrollunits:-1200,1200@32767x0', '32769,2048,4096', 65535, 0, @(-1200, 1200)),
    @('scrollunits:7,0@0x32767', '32769,2048', 0, 65535, @(7)), @('scrollunits:0,-3@0x0', '32769,4096', 0, 0, @(-3)),
    @('scrollunits:1,1@16384x16384', '32769,2048,4096', 32769, 32769, @(1, 1)))) {
    $events = [BridgeVM.BvUnicodeInput]::BuildPointer($case[0])
    $flags = ($events | ForEach-Object { $_.Data.Mouse.Flags }) -join ','
    if ($flags -cne $case[1] -or $events[0].Data.Mouse.X -ne $case[2] -or $events[0].Data.Mouse.Y -ne $case[3] -or
        $events[0].Data.Mouse.MouseData -ne 0) { throw ('scroll units order, flags or move mismatch: ' + $case[0]) }
    $data = @($events | Select-Object -Skip 1 | ForEach-Object { Signed $_ })
    if (($data -join ',') -cne ($case[4] -join ',')) { throw ('scroll units magnitude or sign lost: ' + $case[0]) }
    foreach ($wheel in ($events | Select-Object -Skip 1)) {
        if ($wheel.Type -ne 0 -or $wheel.Data.Mouse.X -ne 0 -or $wheel.Data.Mouse.Y -ne 0) { throw 'scroll units wheel carries a position' }
    }
}
foreach ($command in @('scrollunits:0,0@0x0', 'scrollunits:1201,0@0x0', 'scrollunits:0,-1201@0x0', 'scrollunits:99999,0@0x0',
    'scrollunits:9999999999,0@0x0', 'scrollunits:1@0x0', 'scrollunits:1,2', 'scrollunits:1,2@', 'scrollunits:1,2,3@0x0',
    'scrollunits:+1,0@0x0', 'scrollunits:01,0@0x0', 'scrollunits:-0,1@0x0', 'scrollunits:1, 2@0x0', 'scrollunits:,1@0x0',
    'scrollunits:1,2@32768x0', 'scrollunits:1,2@0x-1', 'scrollunits:1,2@00x0', 'scrollunits:1,2@0x0@0x0', "scrollunits:1,2@0x0`n",
    'scrollunits:1,2@0x0:1', 'scrollunits:1.5,0@0x0', 'scrollunits:1,2@0x0x0', 'ScrollUnits:1,2@0x0', ('scrollunits:1,2@0x' + '0' * 50))) {
    $rejected = $false
    try { $null = [BridgeVM.BvUnicodeInput]::BuildPointer($command) } catch { $rejected = $true }
    if (-not $rejected) { throw ('malformed scroll units accepted: ' + $command) }
}
Write-Output 'PASS: 255 signed UI scroll deltas, raw scroll units on both axes (WHEEL/HWHEEL), coordinate extremes, move-before-wheel and malformed refusal (no SendInput)'
