$ErrorActionPreference = 'Stop'
if (-not ('BridgeVM.BvUnicodeInput' -as [type])) {
    $root = Join-Path $PSScriptRoot 'win-assets'
    Add-Type -Path (Join-Path $root 'bvagent-unicode-input.cs'),(Join-Path $root 'bvagent-key-input.cs'),(Join-Path $root 'bvagent-pointer-input.cs')
}
# One token per INPUT: k<vk>:<KEYEVENTF flags> for a key, m<MOUSEEVENTF flags>@<x>,<y>[/<data>] for the pointer.
function Describe([string]$command) {
    (@([BridgeVM.BvUnicodeInput]::BuildPointer($command)) | ForEach-Object {
        if ($_.Type -eq 1) {
            if ($_.Data.Keyboard.Scan -ne 0 -or $_.Data.Keyboard.Time -ne 0 -or $_.Data.Keyboard.ExtraInfo -ne [UIntPtr]::Zero) { throw "unexpected key fields: $command" }
            'k{0}:{1}' -f $_.Data.Keyboard.VirtualKey, $_.Data.Keyboard.Flags
        } elseif ($_.Type -eq 0) {
            'm{0}@{1},{2}/{3}' -f $_.Data.Mouse.Flags, $_.Data.Mouse.X, $_.Data.Mouse.Y, $_.Data.Mouse.MouseData
        } else { throw "unexpected input type: $command" }
    }) -join ' '
}

# Middle button: ABSOLUTE|MOVE with MIDDLEDOWN 0x20 / MIDDLEUP 0x40; releaseall lifts left, right and middle.
foreach ($case in @(@('middlepress:0x32767', 'm32801@0,65535/0'), @('middlerelease:32767x0', 'm32833@65535,0/0'),
    @('middleclick:0x0', 'm32801@0,0/0 m32833@0,0/0'), @('releaseall:0x0', 'm32853@0,0/0'))) {
    if ((Describe $case[0]) -cne $case[1]) { throw ('middle button flags mismatch: ' + $case[0]) }
}

# Held modifiers: each one down (Win extended) in w c a s order, the unmodified events unchanged, then up in reverse.
$keys = @{ 'w' = 91; 'c' = 17; 'a' = 18; 's' = 16 }
$subsets = 1..15 | ForEach-Object { $bits = $_; -join (0..3 | Where-Object { $bits -band (8 -shr $_) } | ForEach-Object { 'wcas'[$_] }) }
$bases = @('press:1x2', 'release:32767x0', 'releaseall:0x32767', 'rightpress:5x5', 'rightrelease:5x5', 'click:16384x16384',
    'rightclick:0x0', 'middlepress:1x1', 'middlerelease:1x1', 'middleclick:2x3', 'move:7x8', 'wheel:-1', 'scroll:127@0x0',
    'scrollunits:-1200,1200@32767x32767', 'scrollunits:0,-3@1x1', 'scrollunits:5,0@1x1')
$checked = 0
foreach ($base in $bases) {
    foreach ($mods in $subsets) {
        $down = @($mods.ToCharArray() | ForEach-Object { 'k{0}:{1}' -f $keys["$_"], $(if ($_ -eq 'w') { 1 } else { 0 }) })
        $up = @($mods.ToCharArray() | ForEach-Object { 'k{0}:{1}' -f $keys["$_"], $(if ($_ -eq 'w') { 3 } else { 2 }) })
        [array]::Reverse($up)
        $expected = (@($down) + (Describe $base) + @($up)) -join ' '
        if ((Describe ($base + ':' + $mods)) -cne $expected) { throw ('held modifier batch mismatch: ' + $base + ':' + $mods) }
        $checked++
    }
}
if ($checked -ne 240) { throw 'modifier subsets not all checked' }
if ((Describe 'click:32767x0:wcas') -cne 'k91:1 k17:0 k18:0 k16:0 m32771@65535,0/0 m32773@65535,0/0 k16:2 k18:2 k17:2 k91:3') {
    throw 'full modifier batch order mismatch'
}

foreach ($bad in @('press:0x0:', 'press:0x0:S', 'press:0x0:sc', 'press:0x0:ss', 'press:0x0:aw', 'press:0x0:x', 'press:0x0:wcasx',
    'press:0x0:wcass', 'press:0x0: s', 'press:0x0:s ', "press:0x0:s`n", "press:0x0:s`t", 'press:0x0:s:c', 'press:0x0::s',
    'press::s', ':0x0:s', 'press:0x0x0:s', 'press:01x0:s', 'doubleclick:0x0:s', 'press:0x0:1', 'press:0x0:shift', 'press:0x0:+s',
    ('press:0x0:' + [char]0x0455), 'middleclick:0x0x0', 'middleclick:-1x0', 'middleclick:32768x0', 'Middleclick:0x0',
    'middle:0x0', 'middledown:0x0', 'middleclick:0x0:', 'scrollunits:0,0@0x0:s', 'scroll:0@0x0:c', 'wheel:0:c',
    ('click:0x0:' + 'w' * 60))) {
    $blocked = $false
    try { $null = [BridgeVM.BvUnicodeInput]::BuildPointer($bad) } catch { $blocked = $true }
    if (-not $blocked) { throw "malformed modified or middle pointer command accepted: $bad" }
}

# The agent's receipt counts the whole batch, which the host checks against its own count.
. (Join-Path $PSScriptRoot 'win-assets/bvagent-input.ps1')
$id = '12345678-1234-1234-1234-123456789abc'
$encoded = [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes('click:0x0:cs'))
$script:sent = $null
$result = Invoke-UnicodeInput ($id + ' ' + $encoded) { param($text) $script:sent = $text; return 6 } $false $true
if ($result.Exit -ne 0 -or $result.Out -cne "BVINPUT_INSERTED $id 6" -or $script:sent -cne 'click:0x0:cs') { throw 'modified click receipt mismatch' }
$result = Invoke-UnicodeInput ($id + ' ' + $encoded) { param($text) return 2 } $false $true
if ($result.Exit -eq 0) { throw 'a batch inserted without its modifier keys was accepted' }
Write-Output "PASS: middle button flags, $checked held-modifier batches (down, pointer, up reversed), malformed refusal and counted receipt (no SendInput)"
