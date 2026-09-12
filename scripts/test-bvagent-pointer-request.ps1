$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'win-assets/bvagent-input.ps1')
$id = '12345678-1234-1234-1234-123456789abc'
function Request([string]$Text) { return $id + ' ' + [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($Text)) }
$script:calls = 0
$sender = { param($text) $script:calls++; return [uint32]([BridgeVM.BvUnicodeInput]::BuildPointer($text).Length) }
$result = Invoke-UnicodeInput (Request 'click:0x32767') $sender $false $true
if ($result.Exit -ne 0 -or $result.Out -cne "BVINPUT_INSERTED $id 2" -or $script:calls -ne 1) { throw 'pointer receipt/loader mismatch' }
foreach ($count in @(0, 1, 3)) {
    $script:calls = 0
    $script:reported = [uint32]$count
    $result = Invoke-UnicodeInput (Request 'click:0x0') { $script:calls++; return $script:reported } $false $true
    if ($result.Exit -eq 0 -or $script:calls -ne 1 -or -not $result.Out.StartsWith("BVINPUT_FAILED $id ")) {
        throw 'partial/invalid insertion accepted or replayed'
    }
}
foreach ($request in @('invalid YQ==', "$id !!!", "$id /w==", "$id ", "$id YQ== extra",
    (Request 'move:32768x0'), (Request 'wheel:0'), (Request 'ctrl+v'), (Request 'move:0x0:1'),
    (Request ("move:0x0`n")), ('x' * 87422))) {
    $script:calls = 0
    $result = Invoke-UnicodeInput $request $sender $false $true
    if ($result.Exit -eq 0 -or $script:calls -ne 0) { throw 'invalid pointer request reached sender' }
}
$script:calls = 0
$result = Invoke-UnicodeInput (Request 'click:0x0') $sender $true $true
if ($result.Exit -eq 0 -or $script:calls -ne 0) { throw 'ambiguous input mode accepted' }
$script:calls = 0
$result = Invoke-UnicodeInput (Request 'press:1x2') { $script:calls++; throw 'mock-send-failure' } $false $true
if ($result.Exit -eq 0 -or $script:calls -ne 1) { throw 'throwing sender replayed or accepted' }
if ([Environment]::OSVersion.Platform -ne [PlatformID]::Win32NT) {
    $blocked = $false
    try { [BridgeVM.BvUnicodeInput]::InsertPointer('click:0x0') | Out-Null } catch { $blocked = $true }
    if (-not $blocked) { throw 'non-Windows native pointer insertion accepted' }
}
Write-Output 'PASS: pointer request UUID, fresh loader, refusal before send and no retry after partial insertion (no SendInput)'
