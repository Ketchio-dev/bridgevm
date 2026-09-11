$ErrorActionPreference = 'Stop'
$root = Split-Path (Split-Path $PSScriptRoot -Parent) -Parent
$temporary = Join-Path ([IO.Path]::GetTempPath()) ('bridgevm-native-uia-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $temporary | Out-Null
$fixture = $null
try {
    $fixtureScript = Join-Path $temporary 'fixture.ps1'
    $ready = Join-Path $temporary 'ready.txt'
    @'
param([string]$Ready)
Add-Type -AssemblyName System.Windows.Forms
$form = New-Object Windows.Forms.Form
$form.Text = 'BridgeVM native UIA fixture'
$form.Width = 400; $form.Height = 200
$button = New-Object Windows.Forms.Button
$button.Text = 'Got it'; $button.Left = 30; $button.Top = 30; $button.Width = 100
$form.Controls.Add($button)
$form.Add_Shown({ [IO.File]::WriteAllText($Ready, [string]$form.Handle.ToInt64()) })
[Windows.Forms.Application]::Run($form)
'@ | Set-Content -LiteralPath $fixtureScript -Encoding UTF8
    $fixture = Start-Process powershell.exe -ArgumentList @('-Sta', '-NoProfile', '-File', "`"$fixtureScript`"", '-Ready', "`"$ready`"") -PassThru
    $deadline = [DateTime]::UtcNow.AddSeconds(20)
    while (!(Test-Path -LiteralPath $ready) -and [DateTime]::UtcNow -lt $deadline -and !$fixture.HasExited) { Start-Sleep -Milliseconds 100 }
    if (!(Test-Path -LiteralPath $ready)) { throw 'Fixture window did not become ready' }
    $hwndValue = [long](Get-Content -LiteralPath $ready -Raw)
    $probe = Join-Path $root 'scripts/win-assets/bv-b6-native-uia.ps1'
    foreach ($target in @($hwndValue, [long]0)) {
        $stdout = Join-Path $temporary 'probe.out'; $stderr = Join-Path $temporary 'probe.err'
        $child = Start-Process powershell.exe -ArgumentList @('-Mta', '-NoProfile', '-File', "`"$probe`"", '-Hwnd', [string]$target) -PassThru -RedirectStandardOutput $stdout -RedirectStandardError $stderr; $null = $child.Handle # PowerShell issue 5421: retain the handle for ExitCode.
        if (!$child.WaitForExit(20000)) { $child.Kill(); $child.WaitForExit(); throw 'Native COM query exceeded twenty seconds' }
        $child.Refresh()
        $records = @(Get-Content -LiteralPath $stdout | Where-Object { $_.StartsWith('BVNATIVEUIAPROBE ') })
        if ($records.Count -ne 1) { throw "Missing native observation: $(Get-Content -LiteralPath $stderr -Raw)" }
        $record = $records[0].Substring(16) | ConvertFrom-Json
        if (!$record.observation_only -or $record.criterion_pass -or $record.apartment -ne 'MTA') { throw 'Observation identity changed' }
        if ($target -eq 0) {
            if ($child.ExitCode -ne 1 -or $record.status -ne 'query-failed' -or $record.query.exception_type -ne 'System.ArgumentOutOfRangeException') { throw 'Invalid target did not fail closed' }
        } else {
            if ($child.ExitCode -ne 0 -or $record.status -ne 'query-returned' -or $record.stage -ne 'complete') { throw "Native query failed: $($records[0])" }
            if ($record.query.root_process_id -ne $fixture.Id -or $record.query.match_count -ne 1 -or $record.query.truncated) { throw 'Native query did not identify the fixture' }
            $match = $record.query.matches[0]
            if ($match.process_id -ne $fixture.Id -or !$match.is_enabled -or $match.is_offscreen -or $match.bounding_rectangle.Count -ne 4 -or $match.bounding_rectangle[2] -le 0 -or $match.bounding_rectangle[3] -le 0) { throw 'Native button properties invalid' }
        }
    }
} finally {
    if ($null -ne $fixture -and !$fixture.HasExited) { Stop-Process -Id $fixture.Id -Force }
    Remove-Item -LiteralPath $temporary -Recurse -Force
}
Write-Output 'PASS: native UIA COM query observes a real button and rejects invalid targets'; exit 0
