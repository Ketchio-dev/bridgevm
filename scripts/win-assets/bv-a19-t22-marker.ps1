[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [ValidateSet('Launch', 'Read', 'Write')]
    [string]$Action,
    [Parameter(Mandatory = $true)]
    [ValidatePattern('^[0-9a-f]{32}$')]
    [string]$Nonce,
    [string]$Marker = '',
    [string]$WorkAction = '',
    [string]$ExpectedSha256 = ''
)

$ErrorActionPreference = 'Stop'
$Share = 'C:\bridgevm-share'
$MarkerPath = 'C:\bv-snapshot-marker.txt'
$MarkerPattern = '^BV-(ORIGINAL|CLOBBERED|POSTKILL|FINAL)-[0-9a-f]{32}$'

if ($Action -eq 'Launch') {
    if ($WorkAction -cne 'Read' -and $WorkAction -cne 'Write') { throw 'invalid workload action' }
    if ($ExpectedSha256 -cnotmatch '^[0-9a-f]{64}$') { throw 'invalid script digest' }
    if ((Get-FileHash -LiteralPath $PSCommandPath -Algorithm SHA256).Hash.ToLowerInvariant() -cne $ExpectedSha256) {
        throw 'guest script digest mismatch'
    }
    if ($WorkAction -eq 'Write') {
        if ($Marker -cnotmatch $MarkerPattern) { throw 'invalid marker' }
    } elseif ($Marker -cne '') { throw 'read action cannot write a marker' }
    $CommandLine = "powershell.exe -NoProfile -ExecutionPolicy Bypass -File C:\bridgevm-share\bv-a19-t22-marker.ps1 -Action $WorkAction -Nonce $Nonce"
    if ($WorkAction -eq 'Write') { $CommandLine += " -Marker $Marker" }
    $Created = Invoke-CimMethod -ClassName Win32_Process -MethodName Create -Arguments @{ CommandLine = $CommandLine }
    if ($Created.ReturnValue -ne 0 -or $Created.ProcessId -le 0) { throw 'marker workload did not launch' }
    Write-Output "T22-MARKER-LAUNCHED-$Nonce"
    return
}

if ($Action -eq 'Write') {
    if ($Marker -cnotmatch $MarkerPattern) { throw 'invalid marker' }
    Set-Content -NoNewline -Encoding ascii -LiteralPath $MarkerPath -Value $Marker
    $Value = Get-Content -Raw -LiteralPath $MarkerPath
    if ($Value -cne $Marker) { throw 'marker readback mismatch' }
} else {
    if ($Marker -cne '') { throw 'read action cannot write a marker' }
    if (Test-Path -LiteralPath $MarkerPath) {
        $Value = Get-Content -Raw -LiteralPath $MarkerPath
        if ($Value -cnotmatch $MarkerPattern) { throw 'unexpected prior marker' }
    } else {
        $Value = 'BV-NO-MARKER'
    }
}

$Result = Join-Path $Share ("t22-$Nonce-$Action.txt")
$Pending = Join-Path $Share ("t22-$Nonce-$Action.pending")
$Done = Join-Path $Share ("t22-$Nonce-$Action.done")
if ((Test-Path -LiteralPath $Result) -or (Test-Path -LiteralPath $Pending) -or
    (Test-Path -LiteralPath $Done)) { throw 'result name already exists' }
Set-Content -NoNewline -Encoding ascii -LiteralPath $Pending -Value $Value
Move-Item -LiteralPath $Pending -Destination $Result -ErrorAction Stop
$Digest = (Get-FileHash -LiteralPath $Result -Algorithm SHA256).Hash.ToLowerInvariant()
Set-Content -NoNewline -Encoding ascii -LiteralPath $Done -Value $Digest
