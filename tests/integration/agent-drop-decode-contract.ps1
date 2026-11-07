# The guest agent's Finder-drop payload decoder (BridgeVM.BvDropPayload) gives the bytes
# Convert.FromBase64String gives, however the payload line is split across reads, and refuses
# what Convert refuses only after reading the line to its newline. Compiled as C# 5, which is
# what Windows PowerShell 5.1 compiles.
$ErrorActionPreference = 'Stop'
$root = Split-Path (Split-Path $PSScriptRoot -Parent) -Parent
$source = [IO.File]::ReadAllText((Join-Path $root 'scripts/win-assets/bvagent.ps1'))
$found = [regex]::Match($source, "(?s)\`$script:BvDropReaderSource = @'\r?\n(?<cs>.*?)\r?\n'@")
if (-not $found.Success) { throw 'BvDropReaderSource not found in bvagent.ps1' }
$options = if ($PSVersionTable.PSEdition -eq 'Core') { @{ CompilerOptions = '/langversion:5' } } else { @{} }
Add-Type -TypeDefinition $found.Groups['cs'].Value @options

function Feed([byte[]]$line, [int[]]$cuts) {
    # The payload line in reads ending at each cut; the last read also carries the start of the
    # next line, which must be left over.
    $payload = New-Object BridgeVM.BvDropPayload ($line.Length - 1)
    $at = 0
    $ends = @($cuts + $line.Length)
    for ($k = 0; $k -lt $ends.Count; $k++) {
        $cut = $ends[$k]
        $after = if ($k -eq $ends.Count - 1) { 3 } else { 0 }
        $piece = New-Object byte[] ($cut - $at + $after)
        [Array]::Copy($line, $at, $piece, 0, $cut - $at)
        for ($j = 0; $j -lt $after; $j++) { $piece[$cut - $at + $j] = 66 }
        $took = $payload.Take($piece, 0, $piece.Length)
        if ($took -ne $cut - $at) { throw "took $took of a $($cut - $at)-byte read" }
        $at = $cut
    }
    if (-not $payload.Done) { throw 'payload not done at its newline' }
    return $payload
}
function Line([string]$b64) { return , [Text.Encoding]::ASCII.GetBytes($b64 + "`n") }

$rng = New-Object Random 7
$cases = 0
foreach ($size in @(0, 1, 2, 3, 4, 5, 47, 48, 3071, 3072, 3073, 1048576)) {
    $bytes = New-Object byte[] $size
    $rng.NextBytes($bytes)
    $line = Line ([Convert]::ToBase64String($bytes))
    $n = $line.Length - 1
    $splits = @(@(), @([Math]::Min(1, $n)), @([Math]::Max(0, $n - 1)), @([Math]::Max(0, $n - 2)), @([Math]::Max(0, $n - 3)))
    if ($n -gt 8) { $splits += , @(1, 2, 3, 5, 9, [int]($n / 2), ($n - 1)) }
    if ($n -gt 4096) { $splits += , @(4096, 8192, 8193, ($n - 4096)) }
    foreach ($cuts in $splits) {
        $cuts = @($cuts | Sort-Object -Unique | Where-Object { $_ -ge 0 -and $_ -le $n })
        $got = (Feed $line $cuts).Result()
        if ([Convert]::ToBase64String($got) -cne [Convert]::ToBase64String($bytes)) { throw "$size bytes split at $($cuts -join ','): wrong bytes" }
        $cases++
    }
}

# Every refusal is reported only once the line is read to its newline.
foreach ($bad in @('QUJD*', 'QU=D', 'Q===', 'QUJ', 'QUJDR', 'QUJD QUJD', 'QUJD=', 'QUJ=QUJD', "QUJD`r")) {
    $line = [Text.Encoding]::ASCII.GetBytes($bad + "`n")
    $convert = $true
    try { [void][Convert]::FromBase64String($bad) } catch { $convert = $false }
    $payload = Feed $line @()
    $ours = $true
    try { [void]$payload.Result() } catch { $ours = $false }
    if ($ours) { throw "accepted '$bad' (Convert $(if ($convert) { 'accepts' } else { 'refuses' }) it)" }
    $cases++
}
$frame = New-Object BridgeVM.BvDropPayload 4
[void]$frame.Take([Text.Encoding]::ASCII.GetBytes('QUJDX'), 0, 5)
$refused = $false
try { [void]$frame.Result() } catch { $refused = $true }
if (-not $frame.Done -or -not $refused) { throw 'a payload without its newline was accepted' }
$cases++

# Speed, for the record only (the guest's numbers are what count).
$big = New-Object byte[] (1MB); $rng.NextBytes($big)
$line = Line ([Convert]::ToBase64String($big))
$clock = [Diagnostics.Stopwatch]::StartNew()
for ($i = 0; $i -lt 20; $i++) { [void](Feed $line @(4096, 8192, 12288)).Result() }
$ours = $clock.Elapsed.TotalMilliseconds / 20
$text = [Text.Encoding]::ASCII.GetString($line, 0, $line.Length - 1)
$clock.Restart()
for ($i = 0; $i -lt 20; $i++) { [void][Convert]::FromBase64String([Text.Encoding]::ASCII.GetString($line, 0, $line.Length - 1)) }
$convert = $clock.Elapsed.TotalMilliseconds / 20
"agent drop decode contract: PASS ($cases cases; 1 MiB chunk {0:F1} ms, Convert {1:F1} ms)" -f $ours, $convert
