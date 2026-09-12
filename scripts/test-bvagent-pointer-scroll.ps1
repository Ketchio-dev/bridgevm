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
Write-Output 'PASS: 255 signed UI scroll deltas, coordinate extremes, move-before-wheel and malformed refusal (no SendInput)'
