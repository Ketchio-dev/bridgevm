$ErrorActionPreference = 'Stop'
$root = Join-Path $PSScriptRoot 'win-assets'
if (-not ('BridgeVM.BvUnicodeInput' -as [type])) {
    Add-Type -Path (Join-Path $root 'bvagent-unicode-input.cs'),(Join-Path $root 'bvagent-key-input.cs'),(Join-Path $root 'bvagent-pointer-input.cs')
}
. (Join-Path $root 'bvagent-input.ps1')
# Virtual key:flags per event (1 extended, 2 key up): modifiers down in win/ctrl/alt/shift order, the key, then modifiers up in reverse.
$cases = @{
    'enter' = '13:0,13:2'; 'left' = '37:1,37:3'; 'delete' = '46:1,46:3'
    'ctrl+v' = '17:0,86:0,86:2,17:2'; 'shift+tab' = '16:0,9:0,9:2,16:2'; 'shift+enter' = '16:0,13:0,13:2,16:2'; 'shift+space' = '16:0,32:0,32:2,16:2'
    'ctrl+shift+left' = '17:0,16:0,37:1,37:3,16:2,17:2'; 'ctrl+shift+tab' = '17:0,16:0,9:0,9:2,16:2,17:2'; 'shift+f10' = '16:0,121:0,121:2,16:2'; 'shift+f1' = '16:0,112:0,112:2,16:2'; 'shift+delete' = '16:0,46:1,46:3,16:2'
    'alt+f4' = '18:0,115:0,115:2,18:2'; 'ctrl+s' = '17:0,83:0,83:2,17:2'; 'ctrl+w' = '17:0,87:0,87:2,17:2'; 'ctrl+enter' = '17:0,13:0,13:2,17:2'; 'ctrl+tab' = '17:0,9:0,9:2,17:2'
    'win' = '91:1,91:3'; 'alt' = '18:0,18:2'; 'win+e' = '91:1,69:0,69:2,91:3'; 'win+r' = '91:1,82:0,82:2,91:3'; 'win+d' = '91:1,68:0,68:2,91:3'; 'win+v' = '91:1,86:0,86:2,91:3'
    'win+shift+s' = '91:1,16:0,83:0,83:2,16:2,91:3'; 'win+ctrl+d' = '91:1,17:0,68:0,68:2,17:2,91:3'; 'win+left' = '91:1,37:1,37:3,91:3'; 'win+period' = '91:1,190:0,190:2,91:3'
    'ctrl+shift+esc' = '17:0,16:0,27:0,27:2,16:2,17:2'; 'ctrl+esc' = '17:0,27:0,27:2,17:2'; 'alt+s' = '18:0,83:0,83:2,18:2'; 'alt+shift+f' = '18:0,16:0,70:0,70:2,16:2,18:2'; 'alt+space' = '18:0,32:0,32:2,18:2'
    'ctrl+shift+a' = '17:0,16:0,65:0,65:2,16:2,17:2'; 'ctrl+1' = '17:0,49:0,49:2,17:2'; 'ctrl+0' = '17:0,48:0,48:2,17:2'; 'ctrl+f4' = '17:0,115:0,115:2,17:2'
    'ctrl+equal' = '17:0,187:0,187:2,17:2'; 'ctrl+minus' = '17:0,189:0,189:2,17:2'; 'ctrl+slash' = '17:0,191:0,191:2,17:2'; 'ctrl+alt+shift+grave' = '17:0,18:0,16:0,192:0,192:2,16:2,18:2,17:2'
    'win+ctrl+alt+shift+f12' = '91:1,17:0,18:0,16:0,123:0,123:2,16:2,18:2,17:2,91:3'; 'shift+backspace' = '16:0,8:0,8:2,16:2'; 'ctrl+shift+enter' = '17:0,16:0,13:0,13:2,16:2,17:2'; 'insert' = '45:1,45:3'
    'printscreen' = '44:1,44:3'; 'alt+printscreen' = '18:0,44:1,44:3,18:2'; 'win+printscreen' = '91:1,44:1,44:3,91:3'
}
foreach ($command in $cases.Keys) {
    $events = [BridgeVM.BvUnicodeInput]::BuildKey($command)
    $actual = ($events | ForEach-Object { '{0}:{1}' -f $_.Data.Keyboard.VirtualKey, $_.Data.Keyboard.Flags }) -join ','
    if ($actual -cne $cases[$command]) { throw "wrong key event order: $command => $actual" }
    foreach ($event in $events) {
        if ($event.Type -ne 1 -or $event.Data.Keyboard.Scan -ne 0 -or $event.Data.Keyboard.Time -ne 0 -or $event.Data.Keyboard.ExtraInfo -ne [UIntPtr]::Zero) {
            throw 'invalid virtual-key event fields'
        }
    }
}
# A held key: `down:` is a tap's first half (no KEYEVENTF_KEYUP), `up:` its second (all KEYEVENTF_KEYUP); together they are the tap.
foreach ($command in $cases.Keys) {
    $tap = ([BridgeVM.BvUnicodeInput]::BuildKey($command) | ForEach-Object { '{0}:{1}' -f $_.Data.Keyboard.VirtualKey, $_.Data.Keyboard.Flags }) -join ','
    $down = @([BridgeVM.BvUnicodeInput]::BuildKey('down:' + $command)); $up = @([BridgeVM.BvUnicodeInput]::BuildKey('up:' + $command))
    $halves = (@($down) + @($up) | ForEach-Object { '{0}:{1}' -f $_.Data.Keyboard.VirtualKey, $_.Data.Keyboard.Flags }) -join ','
    if ($halves -cne $tap -or $down.Length -ne $up.Length) { throw "held key halves differ from the tap: $command => $halves" }
    if (@($down | Where-Object { $_.Data.Keyboard.Flags -band 2 }).Count -ne 0 -or @($up | Where-Object { -not ($_.Data.Keyboard.Flags -band 2) }).Count -ne 0) { throw "key-up flag outside up: $command" }
}
if ((([BridgeVM.BvUnicodeInput]::BuildKey('down:ctrl+left') + [BridgeVM.BvUnicodeInput]::BuildKey('up:ctrl+left')) | ForEach-Object { '{0}:{1}' -f $_.Data.Keyboard.VirtualKey, $_.Data.Keyboard.Flags }) -join ',' -cne '17:0,37:1,37:3,17:2') { throw 'wrong held chord order' }
foreach ($bad in @('down:', 'up:', 'down:a', 'up:shift+s', 'down:ctrl+alt+delete', 'up:ctrl+alt+delete', 'down:up:left', 'up:down:left', 'hold:left', 'DOWN:left', 'Up:left', 'down: left', 'down:left ', ('down:ctrl+' + 'a' * 28))) {
    $blocked = $false
    try { [BridgeVM.BvUnicodeInput]::BuildKey($bad) | Out-Null } catch { $blocked = $true }
    if (-not $blocked) { throw "unsupported held key accepted: $bad" }
}
# Text keys alone or with only Shift, unknown or non-canonical names, and the secure attention sequence stay refused.
foreach ($bad in @('', 'a', '1', 'CTRL+V', 'Win', 'ctrl+alt+delete', 'win+ctrl+alt+shift+delete', 'shift+ctrl+left', 'ctrl+win+e', 'shift+alt+s', 'ctrl+ctrl+v', 'win+win', 'ctrl+win', 'command+c', 'cmd', "enter`n", 'shift+s', 'shift+1', 'shift+slash', 'shift+f13', 'ctrl+f13', 'ctrl+f0', 'ctrl+', '+', 'ctrl+,', 'ctrl+aa', ('ctrl+' + 'a' * 28))) {
    $blocked = $false
    try { [BridgeVM.BvUnicodeInput]::BuildKey($bad) | Out-Null } catch { $blocked = $true }
    if (-not $blocked) { throw "unsupported chord accepted: $bad" }
}
foreach ($letter in [char[]]'abcdefghijklmnopqrstuvwxyz') { $vk = [int][char]::ToUpperInvariant($letter); if ((([BridgeVM.BvUnicodeInput]::BuildKey('ctrl+' + $letter) | ForEach-Object { '{0}:{1}' -f $_.Data.Keyboard.VirtualKey, $_.Data.Keyboard.Flags }) -join ',') -cne "17:0,${vk}:0,${vk}:2,17:2") { throw "wrong ctrl letter: $letter" } }; $id = '01234567-89AB-CDEF-0123-456789ABCDEF'
$payload = [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes('ctrl+shift+left'))
$result = Invoke-UnicodeInput "$id $payload" { param($key) return [uint32]([BridgeVM.BvUnicodeInput]::BuildKey($key).Length) } $true
if ($result.Exit -ne 0 -or $result.Out -cne "BVINPUT_INSERTED $id 6") { throw 'key receipt mismatch' }
foreach ($held in @(@('down:ctrl+shift+left', 3), @('up:ctrl+shift+left', 3), @('down:left', 1), @('up:f5', 1))) {
    $payload = [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($held[0]))
    $result = Invoke-UnicodeInput "$id $payload" { param($key) return [uint32]([BridgeVM.BvUnicodeInput]::BuildKey($key).Length) } $true
    if ($result.Exit -ne 0 -or $result.Out -cne "BVINPUT_INSERTED $id $($held[1])") { throw "held key receipt mismatch: $($held[0])" }
}
$payload = [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes('win'))
$result = Invoke-UnicodeInput "$id $payload" { param($key) return [uint32]([BridgeVM.BvUnicodeInput]::BuildKey($key).Length) } $true
if ($result.Exit -ne 0 -or $result.Out -cne "BVINPUT_INSERTED $id 2") { throw 'bare Windows key receipt mismatch' }
$result = Invoke-UnicodeInput "$id $payload" { return [uint32]1 } $true
if ($result.Exit -eq 0) { throw 'partial key insertion accepted' }
$payload = [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes('ctrl+alt+delete'))
$script:keySenderCalled = $false
$result = Invoke-UnicodeInput "$id $payload" { $script:keySenderCalled = $true; return [uint32]6 } $true
if ($result.Exit -eq 0 -or $script:keySenderCalled) { throw 'unsupported key reached sender' }
# Held keys overlap like on the VMM's HID report: an up releases only the modifiers no other held action needs (one
# still needed is pressed again instead, so every half keeps its count); a repeat holds once; an up for an action not
# held still releases it (a restarted agent); a down that inserted nothing holds nothing; at most six are held.
$sent = New-Object 'System.Collections.Generic.List[string]'
# The stand-in sender is the agent's own non-generic delegate type: Windows PowerShell 5.1 rejected a generic Func
# literal over the nested Input[] (TypeNotFound), and nested type literals work there (test-bvagent-unicode-input.ps1).
# Pointer events read m<MOUSEEVENTF flags>.
$stub = [BridgeVM.BvUnicodeInput+Sender]({ param($events) $sent.Add((($events | ForEach-Object { if ($_.Type -eq 1) { '{0}:{1}' -f $_.Data.Keyboard.VirtualKey, $_.Data.Keyboard.Flags } else { 'm{0}' -f $_.Data.Mouse.Flags } }) -join ',')); return [uint32]$events.Length }.GetNewClosure())
$none = [BridgeVM.BvUnicodeInput+Sender]{ param($events) return [uint32]0 }
function Assert-Held([string]$command, [string]$expected, $send = $stub) {
    $sent.Clear()
    [void][BridgeVM.BvUnicodeInput]::InsertKey($command, $send)
    if ($sent[0] -cne $expected) { throw "wrong held key events: $command => $($sent[0]) (expected $expected)" }
}
Assert-Held 'down:ctrl+left' '17:0,37:1'
Assert-Held 'down:ctrl+right' '17:0,39:1'
Assert-Held 'down:ctrl+right' '17:0,39:1'
Assert-Held 'ctrl+v' '17:0,86:0,86:2,17:2'
Assert-Held 'up:ctrl+left' '37:3,17:0'
Assert-Held 'up:ctrl+left' '37:3,17:0'
Assert-Held 'up:ctrl+right' '39:3,17:2'
Assert-Held 'up:ctrl+left' '37:3,17:2'
Assert-Held 'down:ctrl+shift+left' '17:0,16:0,37:1'
Assert-Held 'down:shift' '16:0'
Assert-Held 'up:shift' '16:0'
Assert-Held 'up:ctrl+shift+left' '37:3,16:2,17:2'
$failed = $false
try { [void][BridgeVM.BvUnicodeInput]::InsertKey('down:ctrl+up', $none) } catch { $failed = $true }
if (-not $failed) { throw 'a down that inserted nothing was accepted' }
Assert-Held 'down:ctrl+down' '17:0,40:1'
Assert-Held 'up:ctrl+down' '40:3,17:2'
foreach ($n in 1..7) { Assert-Held "down:ctrl+f$n" "17:0,$(111 + $n):0" }
foreach ($n in 1..5) { Assert-Held "up:ctrl+f$n" "$(111 + $n):2,17:0" }
Assert-Held 'up:ctrl+f6' '117:2,17:2'
Assert-Held 'up:ctrl+f7' '118:2,17:2'
Assert-Held 'enter' '13:0,13:2'
$payload = [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes('up:ctrl+left'))
Assert-Held 'down:ctrl+right' '17:0,39:1'
$sent.Clear()
$result = Invoke-UnicodeInput "$id $payload" { param($key) return [BridgeVM.BvUnicodeInput]::InsertKey($key, $stub) } $true
if ($result.Exit -ne 0 -or $result.Out -cne "BVINPUT_INSERTED $id 2" -or $sent.Count -ne 1 -or $sent[0] -cne '37:3,17:0') { throw 'overlapping release receipt mismatch' }
Assert-Held 'up:ctrl+right' '39:3,17:2'
# A modified pointer action leaves down the modifiers a held key needs (pressed again in place of the release), the
# others go up; a plain scroll is unchanged; once the key is up the same scroll releases Ctrl.
function Assert-Pointer([string]$command, [string]$expected) {
    $sent.Clear()
    [void][BridgeVM.BvUnicodeInput]::InsertPointer($command, $stub)
    if ($sent[0] -cne $expected) { throw "wrong pointer events: $command => $($sent[0]) (expected $expected)" }
}
Assert-Held 'down:ctrl+left' '17:0,37:1'
Assert-Pointer 'scroll:-1@0x0:c' '17:0,m32769,m2048,17:0'
Assert-Pointer 'scroll:1@0x0:cs' '17:0,16:0,m32769,m2048,16:2,17:0'
Assert-Pointer 'scroll:-1@0x0' 'm32769,m2048'
Assert-Pointer 'click:0x0:c' '17:0,m32771,m32773,17:0'
Assert-Held 'up:ctrl+left' '37:3,17:2'
Assert-Pointer 'scroll:-1@0x0:c' '17:0,m32769,m2048,17:2'
$agent = [IO.File]::ReadAllText((Join-Path $root 'bvagent.ps1'))
if (-not $agent.Contains("{ `$_ -in @('TEXTINPUT', 'KEYINPUT', 'POINTERINPUT') } { Write-CommandResult `$h (Invoke-UnicodeInput `$arg `$null (`$tok -eq 'KEYINPUT') (`$tok -eq 'POINTERINPUT')) }")) { throw 'key dispatch missing' }
Write-Output 'Guest key chord contracts: PASS (no native input injected)'
