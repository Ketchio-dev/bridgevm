param([Guid]$Nonce)
$ErrorActionPreference = 'Stop'
if ($Nonce -eq [Guid]::Empty) { throw 'fresh nonce required' }
Add-Type -AssemblyName System.Windows.Forms,System.Drawing
$root = Join-Path $env:SystemDrive 'BridgeVM\input-proof'
$id = $Nonce.ToString('D')
$ready = Join-Path $root ('coherence-ready-' + $id + '.json')
$stop = Join-Path $root ('coherence-stop-' + $id)
if ((Test-Path $ready) -or (Test-Path $stop)) { throw 'nonce already used' }
$forms = @()
$timer = New-Object Windows.Forms.Timer
$started = [DateTime]::UtcNow
try {
    foreach ($index in 0..1) {
        $form = New-Object Windows.Forms.Form
        $form.Text = 'BridgeVM Coherence ' + $id + ' ' + $index
        $form.StartPosition = 'Manual'
        $form.Location = New-Object Drawing.Point (20 + 340 * $index),60
        $form.ClientSize = New-Object Drawing.Size 300,180
        $form.ShowInTaskbar = $true
        $forms += $form
        $form.Show()
    }
    $windows = @($forms | ForEach-Object {
        if (-not $_.Visible -or $_.Handle -eq [IntPtr]::Zero) { throw 'fixture window not visible' }
        @{ id = $_.Handle.ToInt64().ToString(); title = $_.Text }
    })
    $value = @{ schema = 'bridgevm.coherence-fixture.v1'; nonce = $id; pid = $PID; windows = $windows }
    [IO.File]::WriteAllText($ready + '.tmp', ($value | ConvertTo-Json -Depth 4 -Compress), (New-Object Text.UTF8Encoding $false))
    Move-Item -LiteralPath ($ready + '.tmp') -Destination $ready
    $timer.Interval = 250
    $timer.Add_Tick({
        if ((Test-Path -LiteralPath $stop) -or ([DateTime]::UtcNow - $started).TotalSeconds -ge 45) {
            $forms[0].Close()
        }
    })
    $timer.Start()
    [Windows.Forms.Application]::Run($forms[0])
} finally {
    $timer.Stop(); $timer.Dispose()
    foreach ($form in $forms) { $form.Dispose() }
}
