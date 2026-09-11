param([string]$Source = (Join-Path $PSScriptRoot 'win-assets/bvagent-unicode-input.cs'))
$ErrorActionPreference = 'Stop'
Add-Type -Path $Source -ErrorAction Stop

function Assert-Equal($Actual, $Expected, [string]$Label) {
    if ($Actual -ne $Expected) { throw "$Label expected=$Expected actual=$Actual" }
}
function Assert-Throws([scriptblock]$Action, [string]$Label) {
    $thrown = $false
    try { & $Action } catch { $thrown = $true }
    if (-not $thrown) { throw "$Label did not fail closed" }
}

Assert-Equal ([IntPtr]::Size) 8 '64-bit test runtime'
Assert-Equal ([Runtime.InteropServices.Marshal]::SizeOf([type][BridgeVM.BvUnicodeInput+KeyboardInput])) 24 'KEYBDINPUT size'
Assert-Equal ([Runtime.InteropServices.Marshal]::SizeOf([type][BridgeVM.BvUnicodeInput+InputUnion])) 32 'INPUT union size'
Assert-Equal ([Runtime.InteropServices.Marshal]::SizeOf([type][BridgeVM.BvUnicodeInput+Input])) 40 'INPUT size'
Assert-Equal ([Runtime.InteropServices.Marshal]::OffsetOf([type][BridgeVM.BvUnicodeInput+Input], 'Data').ToInt64()) 8 'union alignment'
Assert-Equal ([Runtime.InteropServices.Marshal]::OffsetOf([type][BridgeVM.BvUnicodeInput+KeyboardInput], 'ExtraInfo').ToInt64()) 16 'pointer alignment'

$text = 'A' + [char]0xD55C + [char]::ConvertFromUtf32(0x1F642) + 'e' + [char]0x0301
$inputs = [BridgeVM.BvUnicodeInput]::Build($text)
Assert-Equal $inputs.Length ($text.Length * 2) 'one down/up pair per UTF-16 code unit'
for ($i = 0; $i -lt $text.Length; $i++) {
    foreach ($offset in @(0, 1)) {
        $event = $inputs[$i * 2 + $offset]
        Assert-Equal $event.Type 1 'keyboard event'
        Assert-Equal $event.Data.Keyboard.VirtualKey 0 'Unicode virtual key'
        Assert-Equal $event.Data.Keyboard.Scan ([uint16][char]$text[$i]) 'UTF-16 scan'
        Assert-Equal $event.Data.Keyboard.Flags (4 + 2 * $offset) 'down/up flags'
        Assert-Equal $event.Data.Keyboard.Time 0 'system timestamp'
        Assert-Equal $event.Data.Keyboard.ExtraInfo ([UIntPtr]::Zero) 'no extra pointer'
    }
}

Assert-Throws { [BridgeVM.BvUnicodeInput]::Build('') } 'empty input'
Assert-Throws { [BridgeVM.BvUnicodeInput]::Build([string][char]0xD800) } 'unpaired high surrogate'
Assert-Throws { [BridgeVM.BvUnicodeInput]::Build([string][char]0xDC00) } 'unpaired low surrogate'
Assert-Throws { [BridgeVM.BvUnicodeInput]::Build(([string][char]0xD800) + 'a') } 'broken surrogate pair'
Assert-Throws { [BridgeVM.BvUnicodeInput]::Build(('a' * 65537)) } 'oversized input'
Assert-Equal ([BridgeVM.BvUnicodeInput]::Build(('a' * 65536)).Length) 131072 'maximum input'
[BridgeVM.BvUnicodeInput]::RequireComplete(12, 12)
Assert-Throws { [BridgeVM.BvUnicodeInput]::RequireComplete(0, 12) } 'blocked insertion'
Assert-Throws { [BridgeVM.BvUnicodeInput]::RequireComplete(11, 12) } 'partial insertion'
Assert-Throws { [BridgeVM.BvUnicodeInput]::RequireComplete(13, 12) } 'invalid insertion count'
Assert-Throws { [BridgeVM.BvUnicodeInput]::RequireComplete(0, 0) } 'empty insertion receipt'
if ([Environment]::OSVersion.Platform -ne [PlatformID]::Win32NT) {
    Assert-Throws { [BridgeVM.BvUnicodeInput]::Insert('a') } 'non-Windows native call'
}
& (Join-Path $PSScriptRoot 'test-bvagent-unicode-request.ps1')
