function Assert-Hash([string]$Path, [string]$Expected) {
    $item = Get-Item -LiteralPath $Path -ErrorAction Stop
    if ($item.PSIsContainer -or ($item.Attributes -band [IO.FileAttributes]::ReparsePoint)) {
        throw 'B9 source must be a regular file'
    }
    if ((Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant() -cne $Expected) { throw 'B9 source hash mismatch' }
}
function Copy-B9PrivateInput([string]$Source, [string]$Target, [string]$Expected) {
    $item = Get-Item -LiteralPath $Source -ErrorAction Stop
    if ($item.PSIsContainer -or ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -or
        $item.Length -le 0 -or $item.Length -gt 7500000) { throw 'B9 shared input type or size differs' }
    Assert-Hash $Source $Expected
    if (Test-Path -LiteralPath $Target) { throw 'B9 private input already exists' }
    $incoming = [IO.File]::Open($Source, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read)
    try {
        $outgoing = [IO.File]::Open($Target, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
        try { $incoming.CopyTo($outgoing) } finally { $outgoing.Dispose() }
    } finally { $incoming.Dispose() }
    if ((Get-Item -LiteralPath $Target).Length -ne $item.Length) { throw 'B9 private input length differs' }
    Assert-Hash $Target $Expected
    return (Get-FileHash -LiteralPath $Target -Algorithm SHA256).Hash.ToLowerInvariant()
}
function Wait-HostMarker([string]$Name, [int]$Seconds) {
    $path = Join-Path $share $Name
    $deadline = [DateTime]::UtcNow.AddSeconds($Seconds)
    while ([DateTime]::UtcNow -lt $deadline) {
        if (Test-Path -LiteralPath $path -PathType Leaf) {
            try { $raw = [IO.File]::ReadAllText($path, $utf8).Trim() }
            catch [IO.IOException] { $raw = '' }
            if ($raw -ceq $Nonce) { return }
        }
        if ($null -ne $vlc) { $vlc.Refresh(); if ($vlc.HasExited) { throw 'VLC exited before playback was released' } }
        Start-Sleep -Milliseconds 200
    }
    throw 'B9 host marker timeout or nonce mismatch'
}
