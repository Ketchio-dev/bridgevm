$ErrorActionPreference = 'Stop'
$root = Join-Path $PSScriptRoot 'win-assets'
Add-Type -Path (Join-Path $root 'bvagent-unicode-input.cs'),(Join-Path $root 'bvagent-key-input.cs'),(Join-Path $root 'bvagent-pointer-input.cs')
$cases = @{
    'move' = '32769'; 'press' = '32771'; 'release' = '32773'
    'rightpress' = '32777'; 'rightrelease' = '32785'
    'click' = '32771,32773'; 'rightclick' = '32777,32785'
}
foreach ($verb in $cases.Keys) {
    $events = [BridgeVM.BvUnicodeInput]::BuildPointer($verb + ':0x32767')
    $flags = ($events | ForEach-Object { $_.Data.Mouse.Flags }) -join ','
    if ($flags -cne $cases[$verb]) { throw "pointer flags/order mismatch: $verb" }
    foreach ($event in $events) {
        if ($event.Type -ne 0 -or $event.Data.Mouse.X -ne 0 -or $event.Data.Mouse.Y -ne 65535 -or
            $event.Data.Mouse.MouseData -ne 0 -or $event.Data.Mouse.Time -ne 0 -or $event.Data.Mouse.ExtraInfo -ne [UIntPtr]::Zero) {
            throw 'unexpected mouse event fields'
        }
    }
}
$previous = -1
foreach ($coordinate in 0..32767) {
    $event = [BridgeVM.BvUnicodeInput]::BuildPointer(('move:{0}x{0}' -f $coordinate))[0]
    $value = $event.Data.Mouse.X
    if ($value -le $previous -or $value -gt 65535 -or $value -ne $event.Data.Mouse.Y) { throw 'coordinate mapping invalid' }
    $previous = $value
}
foreach ($ticks in @(-127, -1, 1, 127)) {
    $event = [BridgeVM.BvUnicodeInput]::BuildPointer(('wheel:{0}' -f $ticks))[0]
    $signed = [BitConverter]::ToInt32([BitConverter]::GetBytes([uint32]$event.Data.Mouse.MouseData), 0)
    if ($event.Data.Mouse.Flags -ne 2048 -or $signed -ne ($ticks * 120) -or
        $event.Type -ne 0 -or $event.Data.Mouse.X -ne 0 -or $event.Data.Mouse.Y -ne 0) { throw 'wheel encoding invalid' }
}
foreach ($bad in @('', 'move', 'MOVE:0x0', 'move:0x0:1', 'move:-1x0', 'move:32768x0', 'move:0x32768',
    'move:01x0', 'move:+1x0', 'move:0x 1', 'move:1.0x0', "move:0x0`n", 'move:0x0x0',
    'doubleclick:0x0', 'wheel:0', 'wheel:128', 'wheel:-128', 'wheel:+1', 'wheel:01', "wheel:1`n",
    'wheel:1x0', 'wheel:9999999999999999999999', ('move:' + ('1' * 65)))) {
    $blocked = $false
    try { [BridgeVM.BvUnicodeInput]::BuildPointer($bad) | Out-Null } catch { $blocked = $true }
    if (-not $blocked) { throw "invalid pointer command accepted: $bad" }
}
Write-Output 'PASS: native pointer builder, all 32768 coordinates, button order and signed wheel (no SendInput)'
