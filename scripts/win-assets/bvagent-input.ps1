$script:BvInputSourceRoot = $PSScriptRoot

function Invoke-UnicodeInput([string]$Request, [scriptblock]$Sender = $null, [bool]$KeyInput = $false, [bool]$PointerInput = $false) {
    $requestId = 'invalid'
    try {
        if ($Request.Length -gt 87423 -or ($KeyInput -and $PointerInput)) { throw 'unicode-request-length' }
        $parts = $Request.Split(' ')
        $queued = $parts.Length -eq 3 -and $parts[2] -ceq 'S' -and -not $KeyInput -and -not $PointerInput
        if (($parts.Length -ne 2 -and -not $queued) -or $parts[0] -notmatch '^[0-9a-fA-F]{8}(-[0-9a-fA-F]{4}){3}-[0-9a-fA-F]{12}\z') {
            throw 'unicode-request-format'
        }
        $requestId = $parts[0]
        if ($parts[1] -notmatch '^[A-Za-z0-9+/]+={0,2}\z') { throw 'unicode-request-base64' }
        $bytes = [Convert]::FromBase64String($parts[1])
        if ($bytes.Length -eq 0 -or $bytes.Length -gt 65536) { throw 'unicode-request-bytes' }
        $decoder = New-Object System.Text.UTF8Encoding($false, $true)
        $text = $decoder.GetString($bytes)
        if (-not ('BridgeVM.BvUnicodeInput' -as [type])) {
            $sources = @((Join-Path $script:BvInputSourceRoot 'bvagent-unicode-input.cs'),(Join-Path $script:BvInputSourceRoot 'bvagent-key-input.cs'),(Join-Path $script:BvInputSourceRoot 'bvagent-pointer-input.cs'))
            $packets = Join-Path $script:BvInputSourceRoot 'bvagent-text-packets.cs'
            if (Test-Path -LiteralPath $packets) { $sources += $packets }
            Add-Type -Path $sources -ErrorAction Stop
        }
        # Build validates UTF-16 before even a test sender can observe the text.
        $expected = if ($PointerInput) { [uint32]([BridgeVM.BvUnicodeInput]::BuildPointer($text).Length) } elseif ($KeyInput) { [uint32]([BridgeVM.BvUnicodeInput]::BuildKey($text).Length) } else { [uint32]([BridgeVM.BvUnicodeInput]::Build($text).Length) }
        if ($queued) {
            # Unsupported is a positive no-text-dispatch result. The host may then
            # choose the older input path; an unknown outcome is never replayed.
            if ($null -eq $Sender -and ('BridgeVM.BvTextPackets' -as [type]) -and
                [BridgeVM.BvTextPackets]::TryDispatch($text, $script:BvInputSourceRoot)) {
                return @{ Exit = 0; Out = ('BVTEXT_DISPATCHED {0} {1}' -f $requestId, $text.Length) }
            }
            return @{ Exit = 0; Out = ('BVTEXT_UNSUPPORTED {0}' -f $requestId) }
        }
        $inserted = if ($null -ne $Sender) { & $Sender $text } elseif ($PointerInput) { [BridgeVM.BvUnicodeInput]::InsertPointer($text) } elseif ($KeyInput) { [BridgeVM.BvUnicodeInput]::InsertKey($text) } else { [BridgeVM.BvUnicodeInput]::Insert($text) }
        [BridgeVM.BvUnicodeInput]::RequireComplete([uint32]$inserted, $expected)
        return @{ Exit = 0; Out = ('BVINPUT_INSERTED {0} {1}' -f $requestId, $inserted) }
    } catch {
        return @{ Exit = 1; Out = ('BVINPUT_FAILED {0} {1}' -f $requestId, $_.Exception.Message) }
    }
}
