param([string]$Query = (Join-Path $PSScriptRoot '../../scripts/win-assets/bv-b6-tip-point.ps1')); $ErrorActionPreference = 'Stop'
$script = (Resolve-Path $Query).Path
. (Join-Path $PSScriptRoot 'b6-tip-query-process.ps1')
Add-Type -AssemblyName System.Windows.Forms
$work = New-B6TipEvidenceRun; $failed = $true
$form = New-Object System.Windows.Forms.Form
$form.Text = 'B6 native tip contract'; $form.StartPosition = 'Manual'; $form.TopMost = $true
$form.SetBounds(80, 80, 700, 500)
$button = New-Object System.Windows.Forms.Button
$button.Text = 'Got it'; $button.SetBounds(200, 180, 140, 50); $form.Controls.Add($button)
$checks = 0; function Query-Tip {
    param([long]$Target, [string]$Case, [switch]$ExpectedNotFound)
    Invoke-B6TipQuery -Run $work -Query $script -Target $Target -Case $Case -ExpectedNotFound:$ExpectedNotFound -Pump { [System.Windows.Forms.Application]::DoEvents() }
}
try {
    $form.Show(); $form.Activate(); [System.Windows.Forms.Application]::DoEvents()
    $hwnd = $form.Handle.ToInt64()
    $result = Query-Tip $hwnd 'visible-owned'
    if ($result.Code -ne 0 -or $result.Output.Trim() -notmatch "^BVTIPPOINT hwnd=$hwnd state=present x=\d+ y=\d+ owner=$hwnd$") {
        throw "Visible owned button query failed (exit=$($result.Code)): $($result.Error) $($result.Output)"
    }
    $checks++
    $button.Enabled = $false
    $result = Query-Tip $hwnd 'disabled'
    if ($result.Code -eq 0 -or $result.Error -notmatch 'disabled or from a different process') { throw 'Disabled button not rejected for intended reason' }
    $checks++
    $button.Enabled = $true; $button.Text = 'Unrelated button'
    $result = Query-Tip $hwnd 'expected-not-found' -ExpectedNotFound
    if ($result.Code -ne 0 -or $result.Output.Trim() -ne "BVTIPPOINT hwnd=$hwnd state=not-found") { throw 'Nonmatching button was not reported as not-found' }
    $checks++
    $button.Text = 'Got it'
    $second = New-Object System.Windows.Forms.Button
    $second.Text = 'Got it'; $second.SetBounds(360, 180, 140, 50); $form.Controls.Add($second)
    $result = Query-Tip $hwnd 'duplicate'
    if ($result.Code -eq 0 -or $result.Error -notmatch 'Ambiguous visible Got it buttons') { throw 'Duplicate buttons not rejected for intended reason' }
    $checks++
    $form.Hide()
    $result = Query-Tip $hwnd 'hidden-root'
    if ($result.Code -eq 0 -or $result.Error -notmatch 'absent or hidden') { throw 'Hidden root not rejected for intended reason' }
    $checks++
    $failed = $false
} catch { Write-Host ('B6 original failure: ' + $_.Exception.Message); throw }
finally { try { $form.Close(); $form.Dispose() } finally { Complete-B6TipEvidenceRun $work $failed } }
Write-Output "PASS: native tip-point contracts ($checks checks); not a guest or glyph result"
