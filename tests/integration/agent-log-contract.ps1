# The guest agent's log: successful writes add no line, and a log past 8 MB
# moves to bvagent.log.old when the agent starts.
$ErrorActionPreference = 'Stop'
$root = Split-Path (Split-Path $PSScriptRoot -Parent) -Parent
$source = [IO.File]::ReadAllText((Join-Path $root 'scripts/win-assets/bvagent.ps1'))
function Extract([string]$pattern, [string]$what) {
    $found = [regex]::Matches($source, $pattern)
    if ($found.Count -ne 1) { throw "Expected exactly one $what in bvagent.ps1" }
    $found[0].Value
}
$functions = (Extract '(?ms)^function Log\(.*?^}\r?\n' 'Log function') +
    (Extract '(?ms)^function Write-Bytes\(.*?^}\r?\n' 'Write-Bytes function')
$rotation = Extract '(?ms)^# The log keeps.*?^} catch \{ \}\r?\n' 'log rotation'
$dir = Join-Path ([IO.Path]::GetTempPath()) ('bvagent-log-' + [guid]::NewGuid())
New-Item -ItemType Directory $dir | Out-Null
try {
    $bvLog = Join-Path $dir 'bvagent.log'
    . ([scriptblock]::Create($functions))
    function Is-BadHandle($handle) { $false }
    function Gle { 0 }
    Add-Type 'public static class AgentLogFixture { public static bool WriteFile(System.IntPtr h, byte[] b, uint n, ref uint w, System.IntPtr o) { w = n; return true; } }'
    $K = [AgentLogFixture]
    foreach ($i in 1..50) { Write-Bytes ([IntPtr]1) ([Text.Encoding]::ASCII.GetBytes("PONG`n")) 'PONG' }
    if (Test-Path -LiteralPath $bvLog) {
        throw ('50 successful writes logged ' + @(Get-Content -LiteralPath $bvLog).Count + ' lines')
    }
    [IO.File]::WriteAllBytes($bvLog, (New-Object byte[] (9MB)))
    . ([scriptblock]::Create($rotation))
    if ((Test-Path -LiteralPath $bvLog) -or (Get-Item -LiteralPath ($bvLog + '.old')).Length -ne 9MB) {
        throw 'A 9 MB log did not move to bvagent.log.old'
    }
    [IO.File]::WriteAllBytes($bvLog, (New-Object byte[] 1024))
    . ([scriptblock]::Create($rotation))
    if ((Get-Item -LiteralPath $bvLog).Length -ne 1024) { throw 'A small log was moved' }
    'agent log contract: PASS (3 cases)'
} finally {
    Remove-Item -LiteralPath $dir -Recurse -Force -ErrorAction SilentlyContinue
}
