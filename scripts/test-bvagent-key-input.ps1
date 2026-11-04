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
$payload = [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes('win'))
$result = Invoke-UnicodeInput "$id $payload" { param($key) return [uint32]([BridgeVM.BvUnicodeInput]::BuildKey($key).Length) } $true
if ($result.Exit -ne 0 -or $result.Out -cne "BVINPUT_INSERTED $id 2") { throw 'bare Windows key receipt mismatch' }
$result = Invoke-UnicodeInput "$id $payload" { return [uint32]1 } $true
if ($result.Exit -eq 0) { throw 'partial key insertion accepted' }
$payload = [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes('ctrl+alt+delete'))
$script:keySenderCalled = $false
$result = Invoke-UnicodeInput "$id $payload" { $script:keySenderCalled = $true; return [uint32]6 } $true
if ($result.Exit -eq 0 -or $script:keySenderCalled) { throw 'unsupported key reached sender' }
$agent = [IO.File]::ReadAllText((Join-Path $root 'bvagent.ps1'))
if (-not $agent.Contains("{ `$_ -in @('TEXTINPUT', 'KEYINPUT', 'POINTERINPUT') } { Write-CommandResult `$h (Invoke-UnicodeInput `$arg `$null (`$tok -eq 'KEYINPUT') (`$tok -eq 'POINTERINPUT')) }")) { throw 'key dispatch missing' }
Write-Output 'Guest key chord contracts: PASS (no native input injected)'
