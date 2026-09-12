$ErrorActionPreference = 'Stop'
& (Join-Path $PSScriptRoot 'test-bvagent-pointer-scroll.ps1')
. (Join-Path $PSScriptRoot 'win-assets/bvagent-input.ps1')
$events = [BridgeVM.BvUnicodeInput]::BuildPointer('releaseall:32767x0')
if ($events.Length -ne 1 -or $events[0].Type -ne 0 -or $events[0].Data.Mouse.Flags -ne 0x8015 -or
    $events[0].Data.Mouse.X -ne 65535 -or $events[0].Data.Mouse.Y -ne 0) { throw 'common release mismatch' }
if ([BridgeVM.BvUnicodeInput]::BuildPointer('release:0x0')[0].Data.Mouse.Flags -ne 0x8005 -or
    [BridgeVM.BvUnicodeInput]::BuildPointer('rightrelease:0x0')[0].Data.Mouse.Flags -ne 0x8011) {
    throw 'separate button release behavior changed'
}
$id = '12345678-1234-1234-1234-123456789abc'
$encoded = [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes('releaseall:32767x0'))
$script:calls = 0
$result = Invoke-UnicodeInput ($id + ' ' + $encoded) { param($text) $script:calls++; return 1 } $false $true
if ($result.Exit -ne 0 -or $result.Out -cne ('BVINPUT_INSERTED ' + $id + ' 1') -or $script:calls -ne 1) {
    throw 'common release receipt mismatch'
}
foreach ($inserted in @(0, 2)) {
    $script:calls = 0
    $result = Invoke-UnicodeInput ($id + ' ' + $encoded) { param($text) $script:calls++; return $inserted } $false $true
    if ($result.Exit -eq 0 -or $script:calls -ne 1) { throw 'incomplete common release accepted or replayed' }
}
Write-Output 'PASS: common left/right release flags, preserved separate releases and exact correlated count (no SendInput)'
