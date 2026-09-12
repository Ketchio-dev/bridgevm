$ErrorActionPreference = 'Stop'
$path = Join-Path $PSScriptRoot 'win-assets/bv-coherence-multiwindow.ps1'
$tokens = $null; $errors = $null
$ast = [Management.Automation.Language.Parser]::ParseFile($path, [ref]$tokens, [ref]$errors)
if ($errors.Count -ne 0) { throw ($errors | Out-String) }
$text = [IO.File]::ReadAllText($path)
if ($text -match '(?<!\r)\n') { throw 'guest fixture must use CRLF' }
foreach ($required in @('foreach ($index in 0..1)', '$form.Show()', '$_.Handle.ToInt64()',
    'coherence-ready-', 'coherence-stop-', 'bridgevm.coherence-fixture.v1', '.TotalSeconds -ge 45',
    '[Windows.Forms.Application]::Run($forms[0])', '$form.Dispose()')) {
    if (-not $text.Contains($required)) { throw ('missing fixture contract: ' + $required) }
}
if ($text -match '\.Owner\s*=|ShowDialog|EnumWindows|SendInput') { throw 'fixture must not exercise unrelated APIs' }
Write-Output 'PASS: fixture syntax and static contracts only; no real windows or Coherence proven'
