$ErrorActionPreference = 'Stop'
$root = Split-Path (Split-Path $PSScriptRoot -Parent) -Parent
$script = Join-Path $root 'scripts\win-assets\bv-b6-reference-inventory.ps1'
$directory = Join-Path ([IO.Path]::GetTempPath()) ('b6-inventory-' + [Guid]::NewGuid().ToString('N'))
[IO.Directory]::CreateDirectory($directory) | Out-Null
try {
    $output = Join-Path $directory 'inventory.json'
    & $script -OutputPath $output
    $report = Get-Content -LiteralPath $output -Raw -Encoding UTF8 | ConvertFrom-Json
    if ($report.schema_version -ne 1 -or $report.observation_only -ne $true -or $report.reference_accepted -ne $false -or $report.criterion_pass -ne $false) { throw 'Invalid claim boundaries' }
    if (-not $report.os.version -or -not $report.os.build -or -not $report.captured_utc) { throw 'Missing inventory identity' }
    if ((Get-Item -LiteralPath $output).Length -gt 65536) { throw 'Unbounded inventory' }
    foreach ($identity in @($report.d3dconfig, $report.consolas_regular)) {
        if ($identity.present -and $identity.sha256 -notmatch '^[0-9a-f]{64}$') { throw 'Invalid file hash' }
    }
    $before = (Get-FileHash -LiteralPath $output).Hash
    $rejected = $false
    try { & $script -OutputPath $output } catch { $rejected = $true }
    if (-not $rejected -or (Get-FileHash -LiteralPath $output).Hash -ne $before) { throw 'Existing output was not preserved' }
    $rejected = $false
    try { & $script -OutputPath (Join-Path $directory 'missing\inventory.json') } catch { $rejected = $true }
    if (-not $rejected) { throw 'Missing parent accepted' }
    Write-Output 'PASS: Windows inventory, bounded observation-only output, overwrite and missing-parent rejection'
} finally {
    Remove-Item -LiteralPath $directory -Recurse
}
