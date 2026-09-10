$ErrorActionPreference = 'Stop'
$script = Join-Path $PSScriptRoot '../../scripts/win-assets/bv-b6-caption-point.ps1'
Add-Type -AssemblyName System.Windows.Forms
$form = New-Object System.Windows.Forms.Form
$form.Text = 'B6 native caption contract'
$form.StartPosition = 'Manual'
$form.SetBounds(80, 80, 700, 500)
$checks = 0
try {
    $form.Show()
    $form.Activate()
    [System.Windows.Forms.Application]::DoEvents()
    $hwnd = $form.Handle.ToInt64()
    $result = & $script -Hwnd $hwnd -Width 1600 -Height 900
    if ($result -notmatch "^BVCAPTIONPOINT hwnd=$hwnd x=\d+ y=\d+ dpi=\d+ hit=2 owner=$hwnd$") {
        throw "Native caption query did not authenticate target: $result"
    }
    $checks++
    $form.Hide()
    $rejected = $false
    try { & $script -Hwnd $hwnd -Width 1600 -Height 900 | Out-Null } catch { $rejected = $true }
    if (!$rejected) { throw 'Hidden target accepted' }
    $checks++
    $form.FormBorderStyle = 'None'
    $form.Show()
    $form.Activate()
    [System.Windows.Forms.Application]::DoEvents()
    $rejected = $false
    try { & $script -Hwnd $form.Handle.ToInt64() -Width 1600 -Height 900 | Out-Null } catch { $rejected = $true }
    if (!$rejected) { throw 'Captionless target accepted' }
    $checks++
    $rejected = $false
    try { & $script -Hwnd 0 -Width 1600 -Height 900 | Out-Null } catch { $rejected = $true }
    if (!$rejected) { throw 'Null target accepted' }
    $checks++
} finally { $form.Close(); $form.Dispose() }
Write-Output "PASS: native caption-point contracts ($checks checks); not a guest or matrix result"
