[CmdletBinding()]
param([int]$Width = 1600, [int]$Height = 900)

# Deterministic B4 target: one centered button and a large color transition.
# The host prepares the display and validates this ready record before arming
# the input probe. Each click is appended through a separately opened handle
# so the agent can sync the count while the UI process remains alive.
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
$root = 'C:\BridgeVMPtr'
$ready = Join-Path $root 'bv-pointer-target-ready.log'
$clicks = Join-Path $root 'bv-pointer-target-click.log'
Remove-Item $ready,$clicks -Force -ErrorAction SilentlyContinue

$form = New-Object System.Windows.Forms.Form
$form.Text = 'BridgeVM B4 Pointer Target'
$form.FormBorderStyle = [System.Windows.Forms.FormBorderStyle]::None
$screen = [System.Windows.Forms.Screen]::AllScreens | Where-Object { $_.Bounds.Width -eq $Width -and $_.Bounds.Height -eq $Height } | Select-Object -First 1
if (-not $screen) { throw 'B4 target screen not found' }
$virtual = [System.Windows.Forms.SystemInformation]::VirtualScreen
$form.StartPosition = [System.Windows.Forms.FormStartPosition]::Manual
$form.Bounds = $screen.Bounds
$form.BackColor = [System.Drawing.Color]::FromArgb(16, 48, 160)
$form.TopMost = $true

$button = New-Object System.Windows.Forms.Button
$button.Text = 'CLICK BRIDGEVM TARGET'
$button.Font = New-Object System.Drawing.Font('Segoe UI', 28, [System.Drawing.FontStyle]::Bold)
$button.Size = [System.Drawing.Size]::new(600, 300)
$button.Location = [System.Drawing.Point]::new([int](($Width - 600) / 2), [int](($Height - 300) / 2))
$button.BackColor = [System.Drawing.Color]::White
$button.FlatStyle = [System.Windows.Forms.FlatStyle]::Flat
$form.Controls.Add($button)

$count = 0
$button.Add_MouseDown({
    $form.BackColor = [System.Drawing.Color]::FromArgb(32, 208, 64)
    $button.BackColor = [System.Drawing.Color]::FromArgb(255, 224, 32)
    $button.Text = 'PRESS RECEIVED'; $form.Refresh()
})
$button.Add_Click({
    $script:count++
    $button.Text = 'CLICK RECEIVED'; $form.Refresh()
    [IO.File]::AppendAllText($clicks, ('BVTARGET click count=' + $script:count +
        ' utc=' + [DateTime]::UtcNow.ToString('o') + "`r`n"))
})
$form.Add_Shown({
    $form.Activate()
    $button.Focus()
    [IO.File]::WriteAllText($ready, ('BVTARGET ready width=' + $form.ClientSize.Width +
        ' height=' + $form.ClientSize.Height + ' screen_x=' + $screen.Bounds.X + ' screen_y=' + $screen.Bounds.Y +
        ' center_x=' + ($form.Left + ($form.Width / 2)) + ' center_y=' + ($form.Top + ($form.Height / 2)) + ' virtual_x=' + $virtual.X +
        ' virtual_y=' + $virtual.Y + ' virtual_w=' + $virtual.Width + ' virtual_h=' + $virtual.Height +
        ' hwnd=' + $form.Handle + "`r`n"))
})
[void]$form.ShowDialog()
