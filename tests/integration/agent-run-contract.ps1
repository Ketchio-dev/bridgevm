# RUN/PS must wait for their process, not EOF on handles inherited by a sleeping child.
$ErrorActionPreference = 'Stop'
$agent = Join-Path $PSScriptRoot '../../scripts/win-assets/bvagent.ps1'
$tokens = $null; $errors = $null
$ast = [Management.Automation.Language.Parser]::ParseFile($agent, [ref]$tokens, [ref]$errors)
if ($errors.Count) { throw 'agent parse failure' }
# Load definitions only, never the port loop.
foreach ($node in $ast.EndBlock.Statements) {
    if ($node -is [Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -in @('Invoke-B64', 'Initialize-BvCommand')) {
        . ([scriptblock]::Create($node.Extent.Text))
    }
}
if (-not ($IsWindows -or $PSVersionTable.PSEdition -eq 'Desktop')) {
    Write-Output 'SKIP: RUN uses Windows CreateProcess and cmd.exe; sleeping-child regression requires Windows (pwsh and Windows PowerShell 5.1).'
    exit 0
}
function B64([string]$s) { [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($s)) }
$dir = Join-Path ([IO.Path]::GetTempPath()) ('bv-run-test-' + [guid]::NewGuid().ToString('N'))
[void][IO.Directory]::CreateDirectory($dir)
$child = Join-Path $dir 'child.ps1'
$pidFile = Join-Path $dir 'child.pid'
# Inherit stdout/stderr, print the child PID for cleanup, then hold the handles for 12 s.
[IO.File]::WriteAllText($child, "[IO.File]::WriteAllText('$pidFile', [string]`$PID); Start-Sleep -Seconds 12")
try {
    foreach ($ps in @($false, $true)) {
        $spawn = 'powershell.exe -NoProfile -ExecutionPolicy Bypass -File "' + $child + '"'
        $command = if ($ps) {
            '$s=New-Object Diagnostics.ProcessStartInfo; $s.FileName="powershell.exe"; $s.Arguments=' + "'-NoProfile -ExecutionPolicy Bypass -File `"$child`"'" + '; $s.UseShellExecute=$false; [void][Diagnostics.Process]::Start($s); Write-Output before; [Console]::Error.WriteLine("error"); exit 7'
        } else { 'echo before & echo error 1>&2 & start "" /b ' + $spawn + ' & exit /b 7' }
        $watch = [Diagnostics.Stopwatch]::StartNew()
        $reply = Invoke-B64 (B64 $command) $ps
        $watch.Stop()
        if ($watch.Elapsed.TotalSeconds -ge 5) { throw "RUN sleeping child blocked reply: elapsed=$($watch.Elapsed.TotalSeconds) seconds (expected <5)" }
        if ($reply.Exit -ne 7 -or $reply.Out -notmatch 'before' -or $reply.Out -notmatch 'error') { throw "RUN lost output/exit: exit=$($reply.Exit); output=$($reply.Out)" }
        $next = Invoke-B64 (B64 'echo channel-alive') $false
        if ($next.Exit -ne 0 -or $next.Out.Trim() -cne 'channel-alive') { throw 'second RUN did not reply' }
        $until = [DateTime]::UtcNow.AddSeconds(3)
        while (-not (Test-Path $pidFile) -and [DateTime]::UtcNow -lt $until) { Start-Sleep -Milliseconds 20 }
        if (Test-Path $pidFile) { Stop-Process -Id ([int][IO.File]::ReadAllText($pidFile)) -Force -ErrorAction SilentlyContinue; Remove-Item $pidFile }
    }
    $reply = Invoke-B64 (B64 'for /L %i in (1,1,400) do @echo line-%i') $false
    $lines = @($reply.Out -split '\r?\n' | Where-Object { $_ })
    if ($reply.Exit -ne 0 -or $lines.Count -ne 400 -or $lines[399] -cne 'line-400') { throw "RUN truncated output: $($lines.Count) lines" }
    Write-Output 'PASS: RUN and PS ignore sleeping child handles, preserve output and exit, second RUN replies, 400 lines complete'
} finally {
    if (Test-Path $pidFile) { Stop-Process -Id ([int][IO.File]::ReadAllText($pidFile)) -Force -ErrorAction SilentlyContinue }
    Remove-Item $dir -Recurse -Force
}
