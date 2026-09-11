param([string]$Query = (Join-Path $PSScriptRoot '../../scripts/win-assets/bv-b6-tip-point.ps1')); $ErrorActionPreference = 'Stop'
$script = (Resolve-Path $Query).Path
Add-Type -AssemblyName System.Windows.Forms
$work = Join-Path ([IO.Path]::GetTempPath()) ('b6-tip-' + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory $work | Out-Null
$form = New-Object System.Windows.Forms.Form
$form.Text = 'B6 native tip contract'; $form.StartPosition = 'Manual'; $form.TopMost = $true
$form.SetBounds(80, 80, 700, 500)
$button = New-Object System.Windows.Forms.Button
$button.Text = 'Got it'; $button.SetBounds(200, 180, 140, 50); $form.Controls.Add($button)
$checks = 0; function Query-Tip {
    param([long]$Target)
    $id = [Guid]::NewGuid().ToString('N')
    $stdout = Join-Path $work "$id.out"; $stderr = Join-Path $work "$id.err"
    $diagnostic = Join-Path $PSScriptRoot "b6-tip-tree.ps1"
    $arguments = "-NoProfile -ExecutionPolicy Bypass -File `"$diagnostic`" -Query `"$script`" -Hwnd $Target -Width 1600 -Height 900"
    $child = Start-Process -FilePath (Get-Process -Id $PID).Path -ArgumentList $arguments -PassThru -RedirectStandardOutput $stdout -RedirectStandardError $stderr
    try {
        $null = $child.Handle; $deadline = [DateTime]::UtcNow.AddSeconds(20)
        while (!$child.HasExited -and [DateTime]::UtcNow -lt $deadline) {
            [System.Windows.Forms.Application]::DoEvents()
            Start-Sleep -Milliseconds 50
        }
        if (!$child.HasExited) { throw 'Native UIA query timed out' }
        $child.WaitForExit()
        return @{ Code = $child.ExitCode; Output = (Get-Content $stdout -Raw); Error = (Get-Content $stderr -Raw) }
    } finally { if (!$child.HasExited) { $child.Kill(); $child.WaitForExit() }; $child.Dispose() }
}
try {
    $form.Show(); $form.Activate(); [System.Windows.Forms.Application]::DoEvents()
    $hwnd = $form.Handle.ToInt64()
    $result = Query-Tip $hwnd
    if ($result.Code -ne 0 -or $result.Output.Trim() -notmatch "^BVTIPPOINT hwnd=$hwnd state=present x=\d+ y=\d+ owner=$hwnd$") {
        throw "Visible owned button query failed (exit=$($result.Code)): $($result.Error) $($result.Output)"
    }
    $checks++
    $button.Enabled = $false
    $result = Query-Tip $hwnd
    if ($result.Code -eq 0 -or $result.Error -notmatch 'disabled or from a different process') { throw 'Disabled button not rejected for intended reason' }
    $checks++
    $button.Enabled = $true; $button.Text = 'Unrelated button'
    $result = Query-Tip $hwnd
    if ($result.Code -ne 0 -or $result.Output.Trim() -ne "BVTIPPOINT hwnd=$hwnd state=not-found") { throw 'Nonmatching button was not reported as not-found' }
    $checks++
    $button.Text = 'Got it'
    $second = New-Object System.Windows.Forms.Button
    $second.Text = 'Got it'; $second.SetBounds(360, 180, 140, 50); $form.Controls.Add($second)
    $result = Query-Tip $hwnd
    if ($result.Code -eq 0 -or $result.Error -notmatch 'Ambiguous visible Got it buttons') { throw 'Duplicate buttons not rejected for intended reason' }
    $checks++
    $form.Hide()
    $result = Query-Tip $hwnd
    if ($result.Code -eq 0 -or $result.Error -notmatch 'absent or hidden') { throw 'Hidden root not rejected for intended reason' }
    $checks++
} finally { $form.Close(); $form.Dispose(); Remove-Item -Recurse -Force $work }
Write-Output "PASS: native tip-point contracts ($checks checks); not a guest or glyph result"
