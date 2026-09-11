$ErrorActionPreference = 'Stop'
$root = Split-Path (Split-Path $PSScriptRoot -Parent) -Parent
$line = @(Get-Content -LiteralPath (Join-Path $root 'scripts\agent-channel-lib.sh') | Where-Object { $_ -match "^  local command='powershell -NoProfile -Command " })
if ($line.Count -ne 1) { throw 'Expected one readiness producer' }
$match = [regex]::Match($line[0], '^  local command=''powershell -NoProfile -Command "(.*)"''$')
if (-not $match.Success) { throw 'Cannot extract exact readiness body' }
$directory = Join-Path ([IO.Path]::GetTempPath()) ('firstboot-contract-' + [Guid]::NewGuid().ToString('N'))
[IO.Directory]::CreateDirectory($directory) | Out-Null
try {
    foreach ($flag in @($false, $true)) {
        foreach ($taskExit in @(0, 1)) {
            $path = Join-Path $directory 'fixture.ps1'
            $prefix = "function schtasks.exe { `$global:LASTEXITCODE = $taskExit }`nfunction Test-Path { return `$$flag }`n"
            [IO.File]::WriteAllText($path, $prefix + $match.Groups[1].Value, (New-Object Text.UTF8Encoding($false)))
            $output = @(& powershell.exe -NoProfile -File $path)
            $status = $LASTEXITCODE
            $ready = $flag -and ($taskExit -ne 0)
            $expected = if ($ready) { 0 } else { 3 }
            if ($status -ne $expected) { throw "Readiness status changed: flag=$flag taskExit=$taskExit status=$status" }
            if ($output -notcontains "BVFIRSTBOOT_STAGE3_PRESENT=$flag") { throw 'Missing stage3 diagnostic' }
            if ($output -notcontains "BVFIRSTBOOT_TASK_QUERY_EXIT=$taskExit") { throw 'Missing task-query diagnostic' }
            $marker = if ($ready) { 'BVFIRSTBOOT_READY' } else { 'BVFIRSTBOOT_PENDING' }
            if ($output -notcontains $marker) { throw 'Missing readiness marker' }
        }
    }
    Write-Output 'PASS: exact readiness producer diagnostics and unchanged four-case truth table'
    $global:LASTEXITCODE = 0
} finally {
    Remove-Item -LiteralPath $directory -Recurse
}
