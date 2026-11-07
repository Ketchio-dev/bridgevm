# After the agent's CLIPSET, a copy in another guest app must not wait for the agent.
# The agent's thread waits in ReadFile, which pumps no messages; a clipboard owned by
# that thread made every later copy stall about 5 s (live: 5025-5038 ms).
$ErrorActionPreference = 'Stop'
$root = Split-Path (Split-Path $PSScriptRoot -Parent) -Parent
$source = [IO.File]::ReadAllText((Join-Path $root 'scripts/win-assets/bvagent.ps1'))
$helperSource = [regex]::Match($source, "(?s)\r?\n\`$script:BvClipTextSource = @'\r?\n.*?\r?\n'@\r?\n").Value
$setter = [regex]::Match($source, '(?s)\r?\nfunction Set-ClipText\(.*?\r?\n\}\r?\n').Value
if (-not $helperSource -or -not $setter) { throw 'Expected the agent clipboard text helper' }
$holder = $helperSource + $setter + @'
function Log([string]$m) { [Console]::Error.WriteLine($m) }
Set-ClipText 'held-by-agent'
Add-Type -Namespace BvOwner -Name K -MemberDefinition '[DllImport("kernel32.dll")] public static extern System.IntPtr CreateEvent(System.IntPtr a, bool m, bool i, string n); [DllImport("kernel32.dll")] public static extern uint WaitForSingleObject(System.IntPtr h, uint ms);'
[void][BvOwner.K]::WaitForSingleObject([BvOwner.K]::CreateEvent([IntPtr]::Zero, $true, $false, $null), 60000)
'@
$encoded = [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($holder))
$agent = Start-Process powershell -PassThru -WindowStyle Hidden -ArgumentList '-NoProfile', '-STA', '-EncodedCommand', $encoded
try {
    $deadline = (Get-Date).AddSeconds(45)
    while ((Get-Clipboard -Raw) -ne 'held-by-agent') {
        if ((Get-Date) -gt $deadline) { throw 'the agent copy never reached the clipboard' }
        Start-Sleep -Milliseconds 100
    }
    Add-Type -AssemblyName System.Windows.Forms
    $watch = [Diagnostics.Stopwatch]::StartNew()
    [System.Windows.Forms.Clipboard]::SetText('copied-in-an-app')
    $ms = $watch.ElapsedMilliseconds
    if ((Get-Clipboard -Raw) -ne 'copied-in-an-app') { throw 'the app copy did not land' }
    if ($ms -gt 2000) { throw "an app copy after CLIPSET took $ms ms: the agent still owns the clipboard" }
    Write-Output "clipboard owner: PASS (app copy $ms ms)"
} finally {
    Stop-Process -Id $agent.Id -Force -ErrorAction SilentlyContinue
}
