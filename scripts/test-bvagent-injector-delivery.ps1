$ErrorActionPreference = 'Stop'
if ($env:OS -ne 'Windows_NT') { throw 'This contract requires Windows cmd.exe; it is not a WinPE boot.' }
$source = Get-Content -Raw (Join-Path $PSScriptRoot 'win-assets/bvinject.cmd')
$match = [regex]::Match($source, '(?s)rem BVINPUT_COMPANIONS_BEGIN\r?\n(?<body>.*?)\s*rem BVINPUT_COMPANIONS_END')
if (-not $match.Success -or $match.Index -gt $source.IndexOf('copy /y %DRV%\..\bvagent.ps1')) {
    throw 'companion block missing or published after the main agent'
}
$files = @('bvagent-input.ps1', 'bvagent-unicode-input.cs', 'bvagent-key-input.cs', 'bvagent-pointer-input.cs')
$root = Join-Path ([IO.Path]::GetTempPath()) ('BridgeVM injector input ' + [guid]::NewGuid())
New-Item -ItemType Directory -Path $root | Out-Null
try {
    foreach ($scenario in @('complete') + $files + @('locked-destination')) {
        $lane = Join-Path $root $scenario
        $src = Join-Path $lane 'source'; $dst = Join-Path $lane 'destination'
        New-Item -ItemType Directory -Path (Join-Path $src 'drivers'), $dst -Force | Out-Null
        foreach ($file in $files) {
            [IO.File]::WriteAllBytes((Join-Path $src $file), [Text.Encoding]::UTF8.GetBytes(('distinct-' + $file + "`r`n")))
        }
        if ($files -contains $scenario) { Remove-Item (Join-Path $src $scenario) }
        $batch = Join-Path $lane 'run copy block.cmd'
        $body = "@echo off`r`nsetlocal`r`nset `"DRV=$src\drivers`"`r`nset `"WIN=$dst`"`r`n" +
            $match.Groups['body'].Value + "`r`nexit /b 0`r`n:end`r`nexit /b 71`r`n"
        [IO.File]::WriteAllText($batch, $body, [Text.Encoding]::ASCII)
        $lock = $null
        try {
            if ($scenario -eq 'locked-destination') {
                $lock = [IO.File]::Open((Join-Path $dst $files[0]), [IO.FileMode]::OpenOrCreate, [IO.FileAccess]::ReadWrite, [IO.FileShare]::None)
            }
            & $env:ComSpec /d /c ('call "' + $batch + '"') | Out-Null
            $status = $LASTEXITCODE
        } finally { if ($null -ne $lock) { $lock.Dispose() } }
        if ($scenario -eq 'complete') {
            if ($status -ne 0) { throw 'complete companion copy failed' }
            foreach ($file in $files) {
                if ((Get-FileHash (Join-Path $src $file)).Hash -cne (Get-FileHash (Join-Path $dst $file)).Hash) {
                    throw ('companion bytes differ: ' + $file)
                }
            }
        } else {
            if ($status -ne 71) { throw ('plant continued after failure: ' + $scenario) }
            $expected = if ($scenario -eq 'locked-destination') { 1 } else { 0 }
            if (@(Get-ChildItem -LiteralPath $dst).Count -ne $expected) { throw 'copy continued after preflight or write failure' }
        }
    }
} finally { Remove-Item -LiteralPath $root -Recurse -Force }
Write-Output 'PASS: real cmd companion copies, four missing-source refusals and locked-destination abort (not a WinPE boot)'
