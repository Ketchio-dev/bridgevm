$script:BvInputSourceRoot = $PSScriptRoot

function Invoke-UnicodeInput([string]$Request, [scriptblock]$Sender = $null, [bool]$KeyInput = $false, [bool]$PointerInput = $false) {
    $requestId = 'invalid'
    try {
        if ($Request.Length -gt 87421 -or ($KeyInput -and $PointerInput)) { throw 'unicode-request-length' }
        $parts = $Request.Split(' ')
        if ($parts.Length -ne 2 -or $parts[0] -notmatch '^[0-9a-fA-F]{8}(-[0-9a-fA-F]{4}){3}-[0-9a-fA-F]{12}\z') {
            throw 'unicode-request-format'
        }
        $requestId = $parts[0]
        if ($parts[1] -notmatch '^[A-Za-z0-9+/]+={0,2}\z') { throw 'unicode-request-base64' }
        $bytes = [Convert]::FromBase64String($parts[1])
        if ($bytes.Length -eq 0 -or $bytes.Length -gt 65536) { throw 'unicode-request-bytes' }
        $decoder = New-Object System.Text.UTF8Encoding($false, $true)
        $text = $decoder.GetString($bytes)
        if (-not ('BridgeVM.BvUnicodeInput' -as [type])) {
            Add-Type -Path (Join-Path $script:BvInputSourceRoot 'bvagent-unicode-input.cs'),(Join-Path $script:BvInputSourceRoot 'bvagent-key-input.cs'),(Join-Path $script:BvInputSourceRoot 'bvagent-pointer-input.cs') -ErrorAction Stop
        }
        # Build validates UTF-16 before even a test sender can observe the text.
        $expected = if ($PointerInput) { [uint32]([BridgeVM.BvUnicodeInput]::BuildPointer($text).Length) } elseif ($KeyInput) { [uint32]([BridgeVM.BvUnicodeInput]::BuildKey($text).Length) } else { [uint32]([BridgeVM.BvUnicodeInput]::Build($text).Length) }
        $inserted = if ($null -ne $Sender) { & $Sender $text } elseif ($PointerInput) { [BridgeVM.BvUnicodeInput]::InsertPointer($text) } elseif ($KeyInput) { [BridgeVM.BvUnicodeInput]::InsertKey($text) } else { [BridgeVM.BvUnicodeInput]::Insert($text) }
        [BridgeVM.BvUnicodeInput]::RequireComplete([uint32]$inserted, $expected)
        return @{ Exit = 0; Out = ('BVINPUT_INSERTED {0} {1}' -f $requestId, $inserted) }
    } catch {
        return @{ Exit = 1; Out = ('BVINPUT_FAILED {0} {1}' -f $requestId, $_.Exception.Message) }
    }
}
