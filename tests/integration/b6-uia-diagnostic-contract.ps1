$ErrorActionPreference = 'Stop'
$root = (Resolve-Path (Join-Path $PSScriptRoot '../..')).Path
$work = Join-Path ([IO.Path]::GetTempPath()) ('b6-uia-' + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory $work | Out-Null
try {
    $wrapper = Join-Path $work 'bv-b6-uia-diagnostic.ps1'
    Copy-Item (Join-Path $root 'scripts/win-assets/bv-b6-uia-diagnostic.ps1') $wrapper
    $query = Join-Path $work 'bv-b6-tip-point.ps1'
    foreach ($mode in @('Sta', 'Mta')) {
        foreach ($fail in @($false, $true)) {
            $stub = 'param($Hwnd,$Width,$Height); Write-Output "BVTIPPOINT hwnd=$Hwnd state=not-found"'
            if ($fail) { $stub = 'throw [Runtime.InteropServices.COMException]::new("fixture", -2147418113)' }
            Set-Content -Path $query -Value $stub -Encoding ASCII
            $raw = & powershell.exe "-$mode" -NoProfile -NonInteractive -ExecutionPolicy Bypass -File $wrapper -Hwnd 42 -Width 1600 -Height 900
            $code = $LASTEXITCODE
            $records = @($raw | Where-Object { $_ -like 'BVUIAPROBE *' })
            if ($records.Count -ne 1) { throw 'Expected exactly one diagnostic record' }
            $record = $records[0].Substring(11) | ConvertFrom-Json
            if (!$record.observation_only -or $record.criterion_pass -or $record.apartment -ne $mode.ToUpperInvariant() -or $record.hwnd -ne '42') { throw 'Bad diagnostic identity' }
            if ($fail) {
                if ($code -ne 1 -or $record.status -ne 'query-failed' -or @($record.exception_chain | Where-Object { $_.hresult -eq -2147418113 }).Count -eq 0) { throw 'Lost query failure' }
            } elseif ($code -ne 0 -or $record.status -ne 'query-returned' -or $record.query_output[0] -ne 'BVTIPPOINT hwnd=42 state=not-found') { throw 'Lost query result' }
        }
    }
} finally {
    Remove-Item -Recurse -Force $work
}
Write-Output 'PASS: STA/MTA diagnostic reporting preserves results and COM failures'
