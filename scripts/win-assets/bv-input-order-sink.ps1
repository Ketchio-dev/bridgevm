param([Guid]$Nonce = [Guid]::Empty, [switch]$ValidateOnly)
$ErrorActionPreference = 'Stop'
$script:FirstText = 'BridgeVM'
$script:SecondText = [string][char]0xD55C + [char]0xAE00 + [char]0xD83D + [char]0xDE42
$script:ExpectedText = $script:FirstText + "`r`n" + $script:SecondText

function Get-BvInputProof([hashtable]$State, [string]$Text, [bool]$Clicked) {
    $hash = [System.Security.Cryptography.SHA256]::Create()
    try { $digest = [BitConverter]::ToString($hash.ComputeHash([Text.Encoding]::UTF8.GetBytes($Text))).Replace('-', '').ToLowerInvariant() }
    finally { $hash.Dispose() }
    $matched = $Text -ceq $script:ExpectedText
    $passed = $Clicked -and $matched -and $State.EnterCount -eq 1 -and $State.EnterSawFirstText -and -not $State.FocusLost
    return [ordered]@{ schema = 'bridgevm.input-sink.v1'; nonce = $State.Nonce; passed = [bool]$passed;
        clicked = $Clicked; text_matches = $matched; enter_count = $State.EnterCount;
        enter_saw_first_text = $State.EnterSawFirstText; focus_lost = $State.FocusLost; actual_text_sha256 = $digest }
}

if (-not ('BridgeVM.InputSinkFocus' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
namespace BridgeVM {
  public static class InputSinkFocus {
    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] [return: MarshalAs(UnmanagedType.Bool)]
    public static extern bool SetForegroundWindow(IntPtr window);
  }
}
'@
}
if ($ValidateOnly) { return }
if ($Nonce -eq [Guid]::Empty) { throw 'A fresh nonempty request nonce is required.' }
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
$root = Join-Path $env:SystemDrive 'BridgeVM\input-proof'
[IO.Directory]::CreateDirectory($root) | Out-Null
$id = $Nonce.ToString('D')
$script:ReadyPath = Join-Path $root ('ready-' + $id + '.json')
$script:ResultPath = Join-Path $root ('result-' + $id + '.json')
if ((Test-Path $script:ReadyPath) -or (Test-Path $script:ResultPath)) { throw 'Refusing reused input proof nonce.' }
$script:State = @{ Nonce = $id; EnterCount = 0; EnterSawFirstText = $false; FocusLost = $false; Ready = $false; Done = $false }

function Write-BvInputJson([string]$Path, $Value) {
    [IO.File]::WriteAllText($Path + '.tmp', ($Value | ConvertTo-Json -Compress), (New-Object Text.UTF8Encoding($false)))
    Move-Item -LiteralPath ($Path + '.tmp') -Destination $Path
}
function Finish-BvInputProof([string]$Reason, [bool]$Clicked) {
    if ($script:State.Done) { return }
    $script:State.Done = $true
    $script:Timer.Stop()
    $proof = Get-BvInputProof $script:State $script:Box.Text $Clicked
    $proof.reason = $Reason
    try { Write-BvInputJson $script:ResultPath $proof }
    finally { $script:Form.Close() }
}

$script:Form = New-Object Windows.Forms.Form
$script:Form.Text = 'BridgeVM ordered input diagnostic'
$script:Form.ClientSize = New-Object Drawing.Size(600, 350)
$script:Form.StartPosition = 'Manual'
$script:Form.Location = New-Object Drawing.Point(0, 0)
$script:Form.TopMost = $true
$script:Box = New-Object Windows.Forms.TextBox
$script:Box.Location = New-Object Drawing.Point(20, 20)
$script:Box.Size = New-Object Drawing.Size(560, 160)
$script:Box.Multiline = $true
$script:Box.AcceptsReturn = $true
$button = New-Object Windows.Forms.Button
$button.Text = 'Finish input proof'
$button.Location = New-Object Drawing.Point(20, 230)
$button.Size = New-Object Drawing.Size(300, 65)
$script:Form.Controls.Add($script:Box)
$script:Form.Controls.Add($button)
$script:Box.add_KeyDown({
    if ($_.KeyCode -eq [Windows.Forms.Keys]::Enter) {
        $script:State.EnterCount++
        $script:State.EnterSawFirstText = $script:Box.Text -ceq $script:FirstText
    }
})
$button.add_Click({ Finish-BvInputProof 'clicked' $true })
$script:Form.add_Deactivate({ if ($script:State.Ready) { $script:State.FocusLost = $true } })
$script:Form.add_FormClosed({ if (-not $script:State.Done) { Finish-BvInputProof 'closed' $false } })
$script:Timer = New-Object Windows.Forms.Timer
$script:Timer.Interval = 45000
$script:Timer.add_Tick({ Finish-BvInputProof 'timeout' $false })
$script:Form.add_Shown({
    $script:Form.Activate()
    $script:Box.Focus() | Out-Null
    [BridgeVM.InputSinkFocus]::SetForegroundWindow($script:Form.Handle) | Out-Null
    if ([BridgeVM.InputSinkFocus]::GetForegroundWindow() -ne $script:Form.Handle) { Finish-BvInputProof 'focus-unavailable' $false; return }
    $point = $button.PointToScreen((New-Object Drawing.Point(150, 32)))
    $screen = [Windows.Forms.Screen]::PrimaryScreen.Bounds
    $x = [int][Math]::Round(($point.X - $screen.X) * 32767.0 / ($screen.Width - 1))
    $y = [int][Math]::Round(($point.Y - $screen.Y) * 32767.0 / ($screen.Height - 1))
    if ($x -lt 0 -or $x -gt 32767 -or $y -lt 0 -or $y -gt 32767) { Finish-BvInputProof 'button-offscreen' $false; return }
    $script:State.Ready = $true
    Write-BvInputJson $script:ReadyPath ([ordered]@{ schema = 'bridgevm.input-sink-ready.v1'; nonce = $id;
        foreground = $true; button_x = $x; button_y = $y; first_text = $script:FirstText;
        second_text_base64 = [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($script:SecondText)) })
})
$script:Timer.Start()
try { $script:Form.ShowDialog() | Out-Null }
finally { $script:Timer.Dispose(); $script:Form.Dispose() }
