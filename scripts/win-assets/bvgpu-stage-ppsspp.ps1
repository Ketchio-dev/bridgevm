param(
    [Parameter(Mandatory = $true)][ValidatePattern('^[a-z0-9-]+[.]zip[.]part-$')][string]$ArchivePrefix,
    [Parameter(Mandatory = $true)][ValidateRange(1, 64)][int]$ChunkCount,
    [Parameter(Mandatory = $true)][ValidatePattern('^[0-9a-f]{64}$')][string]$ExpectedPayloadSha256,
    [Parameter(Mandatory = $true)][ValidatePattern('^[0-9a-f]{64}$')][string]$ExpectedExecutableSha256
)
$ErrorActionPreference = 'Stop'
$share = 'C:\BridgeVMShare'
$archive = Join-Path $env:TEMP ('bridgevm-ppsspp-' + $ExpectedPayloadSha256 + '.zip')
$staging = 'C:\BridgeVM\a2-title.staging'
$target = 'C:\BridgeVM\a2-title'
Remove-Item -Force -ErrorAction SilentlyContinue $archive
$out = [IO.File]::Open($archive, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
try {
    for ($index = 0; $index -lt $ChunkCount; $index++) {
        $part = Join-Path $share ($ArchivePrefix + $index.ToString('000'))
        if (-not (Test-Path -LiteralPath $part -PathType Leaf)) { throw "missing payload chunk $index" }
        $input = [IO.File]::OpenRead($part)
        try { $input.CopyTo($out) } finally { $input.Dispose() }
    }
    $out.Flush($true)
} finally { $out.Dispose() }
$payloadHash = (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant()
if ($payloadHash -cne $ExpectedPayloadSha256) { throw 'PPSSPP payload hash mismatch' }
Remove-Item -Recurse -Force -ErrorAction SilentlyContinue $staging
Expand-Archive -LiteralPath $archive -DestinationPath $staging -Force
$executable = Join-Path $staging 'ppsspp\PPSSPPWindowsARM64.exe'
if (-not (Test-Path -LiteralPath $executable -PathType Leaf)) { throw 'PPSSPP ARM64 executable absent' }
$executableHash = (Get-FileHash -LiteralPath $executable -Algorithm SHA256).Hash.ToLowerInvariant()
if ($executableHash -cne $ExpectedExecutableSha256) { throw 'PPSSPP executable hash mismatch' }
Remove-Item -Recurse -Force -ErrorAction SilentlyContinue $target
Move-Item -LiteralPath $staging -Destination $target
Remove-Item -Force $archive
Write-Output 'prep=PPSSPPPAYLOADOK'
