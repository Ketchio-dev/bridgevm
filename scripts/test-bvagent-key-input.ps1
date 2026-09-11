$ErrorActionPreference = 'Stop'
$root = Join-Path $PSScriptRoot 'win-assets'
if (-not ('BridgeVM.BvUnicodeInput' -as [type])) {
    Add-Type -Path (Join-Path $root 'bvagent-unicode-input.cs'),(Join-Path $root 'bvagent-key-input.cs')
}
. (Join-Path $root 'bvagent-input.ps1')
$cases = @{
    'enter' = '13:0,13:2'; 'left' = '37:1,37:3'; 'delete' = '46:1,46:3'
    'ctrl+v' = '17:0,86:0,86:2,17:2'; 'shift+tab' = '16:0,9:0,9:2,16:2'
    'ctrl+shift+left' = '17:0,16:0,37:1,37:3,16:2,17:2'
    'alt+f4' = '18:0,115:0,115:2,18:2'
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
foreach ($bad in @('', 'a', 'CTRL+V', 'ctrl+alt+delete', 'shift+ctrl+left', 'ctrl+ctrl+v', 'command+c', "enter`n")) {
    $blocked = $false
    try { [BridgeVM.BvUnicodeInput]::BuildKey($bad) | Out-Null } catch { $blocked = $true }
    if (-not $blocked) { throw "unsupported chord accepted: $bad" }
}
$id = '01234567-89AB-CDEF-0123-456789ABCDEF'
$payload = [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes('ctrl+shift+left'))
$result = Invoke-UnicodeInput "$id $payload" { param($key) return [uint32]([BridgeVM.BvUnicodeInput]::BuildKey($key).Length) } $true
if ($result.Exit -ne 0 -or $result.Out -cne "BVINPUT_INSERTED $id 6") { throw 'key receipt mismatch' }
$result = Invoke-UnicodeInput "$id $payload" { return [uint32]1 } $true
if ($result.Exit -eq 0) { throw 'partial key insertion accepted' }
$payload = [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes('ctrl+alt+delete'))
$script:keySenderCalled = $false
$result = Invoke-UnicodeInput "$id $payload" { $script:keySenderCalled = $true; return [uint32]6 } $true
if ($result.Exit -eq 0 -or $script:keySenderCalled) { throw 'unsupported key reached sender' }
$agent = [IO.File]::ReadAllText((Join-Path $root 'bvagent.ps1'))
if (-not $agent.Contains("'KEYINPUT' { Write-CommandResult `$h (Invoke-UnicodeInput `$arg `$null `$true) }")) { throw 'key dispatch missing' }
Write-Output 'Guest editing key contracts: PASS (no native input injected)'
