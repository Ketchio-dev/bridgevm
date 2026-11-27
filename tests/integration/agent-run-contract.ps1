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
# Inherit stdout/stderr, print the child PID for cleanup, then hold the handles for 60 s: a RUN
# that waits for them takes a minute, while a correct one replies in seconds even on a slow hosted
# runner (5.3-6.3 s seen, which failed the earlier 5 s bound).
[IO.File]::WriteAllText($child, "[IO.File]::WriteAllText('$pidFile', [string]`$PID); Start-Sleep -Seconds 60")
try {
    foreach ($ps in @($false, $true)) {
        $spawn = 'powershell.exe -NoProfile -ExecutionPolicy Bypass -File "' + $child + '"'
        $command = if ($ps) {
            '$s=New-Object Diagnostics.ProcessStartInfo; $s.FileName="powershell.exe"; $s.Arguments=' + "'-NoProfile -ExecutionPolicy Bypass -File `"$child`"'" + '; $s.UseShellExecute=$false; [void][Diagnostics.Process]::Start($s); Write-Output before; [Console]::Error.WriteLine("error"); exit 7'
        } else { 'echo before & echo error 1>&2 & start "" /b ' + $spawn + ' & exit /b 7' }
        $watch = [Diagnostics.Stopwatch]::StartNew()
        $reply = Invoke-B64 (B64 $command) $ps
        $watch.Stop()
        if ($watch.Elapsed.TotalSeconds -ge 30) { throw "RUN sleeping child blocked reply: elapsed=$($watch.Elapsed.TotalSeconds) seconds (expected <30)" }
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
    # A command that prompts must read end of input, not the agent's stdin: in the guest that is a
    # hidden console nobody types into, so a prompt waited forever and the host's RUN never got
    # its reply. Run the agent's functions in a child whose stdin is a pipe nobody writes to.
    $defs = Join-Path $dir 'defs.ps1'
    $texts = foreach ($node in $ast.EndBlock.Statements) {
        if ($node -is [Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -in @('Invoke-B64', 'Initialize-BvCommand')) { $node.Extent.Text }
    }
    [IO.File]::WriteAllText($defs, ($texts -join "`r`n"))
    $probe = Join-Path $dir 'prompt.ps1'
    [IO.File]::WriteAllText($probe, @'
. $args[0]
function B64([string]$s) { [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($s)) }
$c = Invoke-B64 (B64 'set /p v=Value: & echo cmd-after') $false
Write-Output ('cmd=' + ($c.Out -replace '\s+', ' '))
$p = Invoke-B64 (B64 "Read-Host 'Value'; 'ps-after'") $true
Write-Output ('ps=' + ($p.Out -replace '\s+', ' '))
'@)
    $start = New-Object Diagnostics.ProcessStartInfo
    $start.FileName = (Get-Process -Id $PID).Path
    $start.Arguments = '-NoProfile -ExecutionPolicy Bypass -File "' + $probe + '" "' + $defs + '"'
    $start.UseShellExecute = $false; $start.RedirectStandardInput = $true; $start.RedirectStandardOutput = $true
    $prompting = [Diagnostics.Process]::Start($start)
    $read = $prompting.StandardOutput.ReadToEndAsync()
    if (-not $prompting.WaitForExit(60000)) {
        Get-CimInstance Win32_Process -Filter "ParentProcessId=$($prompting.Id)" | ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }
        $prompting.Kill()
        throw 'RUN/PS with a prompt waited for input instead of replying'
    }
    $printed = $read.Result
    if ($printed -notmatch 'cmd=.*cmd-after' -or $printed -notmatch 'ps=') { throw "prompting RUN/PS lost output: $printed" }
    Write-Output 'PASS: RUN and PS ignore sleeping child handles, preserve output and exit, second RUN replies, 400 lines complete, prompts read end of input'
} finally {
    if (Test-Path $pidFile) { Stop-Process -Id ([int][IO.File]::ReadAllText($pidFile)) -Force -ErrorAction SilentlyContinue }
    Remove-Item $dir -Recurse -Force
}
