# Windows links for the Mac browser: the handler the app registers (HvfGuestLinks.swift handlerScript), the
# agent's read of its link file (bvagent.ps1 BvClipWatch.TakeLinks) and Read-Line's OPENURL/CLIPCHANGED
# dispatch. No registry, no SendInput, no browser: Start-Process is a stub.
param([ValidateSet('all', 'readline')][string]$Mode = 'all', [string]$Agent = '')
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
if ($Agent -eq '') { $Agent = Join-Path $PSScriptRoot 'win-assets/bvagent.ps1' }
$onWindows = [Environment]::OSVersion.Platform -eq 'Win32NT'
$tokens = $null; $errors = $null
$ast = [System.Management.Automation.Language.Parser]::ParseFile($Agent, [ref]$tokens, [ref]$errors)
if ($errors.Count -ne 0) { throw 'agent parse failed' }
function Get-AgentFunction([string]$name) {
    $node = $ast.Find({ param($n) $n -is [System.Management.Automation.Language.FunctionDefinitionAst] -and $n.Name -eq $name }, $true)
    if ($null -eq $node) { throw "missing agent function $name" }
    return $node.Extent.Text
}

if ($Mode -eq 'readline') {
    # Read-Line with scripted reads: what it writes between replies with the clipboard and link watches on or off.
    Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Text;
namespace BridgeVM {
    public static class BvClipWatch {
        public static Queue<int> Reads = new Queue<int>();
        public static int ReadCalls;
        public static string[] Links = new string[0];
        public static int Read(IntPtr h, byte[] buf) {
            ReadCalls++;
            int r = Reads.Count > 0 ? Reads.Dequeue() : 0;
            if (r <= 0) { return r; }
            byte[] line = Encoding.ASCII.GetBytes("PING\n");
            Array.Copy(line, buf, line.Length);
            return line.Length;
        }
        public static string[] TakeLinks() { return Links; }
    }
    public static class FakeKernel {
        public static int ReadFileCalls;
        public static bool ReadFile(IntPtr h, byte[] buf, uint toRead, out uint read, IntPtr overlapped) {
            ReadFileCalls++;
            byte[] line = Encoding.ASCII.GetBytes("PING\n");
            Array.Copy(line, buf, line.Length);
            read = (uint)line.Length;
            return true;
        }
    }
}
'@
    $K = [BridgeVM.FakeKernel]
    function Is-BadHandle($handle) { return $false }
    function Write-Line($handle, [string]$line, [string]$what) { [void]$script:written.Add($line) }
    . ([scriptblock]::Create((Get-AgentFunction 'Read-Line')))
    $link = 'aHR0cHM6Ly9leGFtcGxlLmNvbS8='
    $cases = @(
        @('off', $false, $false, @(), ''),
        @('clipboard', $true, $false, @(-1, 5), 'CLIPCHANGED'),
        @('links', $false, $true, @(-1, -2, 5), "OPENURL $link"),
        @('both', $true, $true, @(-2, -1, 5), "OPENURL $link|CLIPCHANGED"))
    foreach ($case in $cases) {
        $script:BvClipPush = $case[1]; $script:BvLinkWatch = $case[2]
        $script:BvReadBuffer = New-Object byte[] 16384
        $script:BvPending = New-Object System.Text.StringBuilder
        $script:BvLines = New-Object 'System.Collections.Generic.Queue[string]'
        $script:written = New-Object System.Collections.ArrayList
        [BridgeVM.BvClipWatch]::Reads.Clear(); foreach ($r in $case[3]) { [BridgeVM.BvClipWatch]::Reads.Enqueue($r) }
        [BridgeVM.BvClipWatch]::Links = @($link)
        [BridgeVM.BvClipWatch]::ReadCalls = 0; [BridgeVM.FakeKernel]::ReadFileCalls = 0
        $line = Read-Line ([IntPtr]5)
        $wrote = $script:written -join '|'
        if ($line -cne 'PING' -or $wrote -cne $case[4]) { throw "Read-Line $($case[0]): returned '$line', wrote '$wrote'" }
        $watched = $case[1] -or $case[2]
        if (($watched -and [BridgeVM.FakeKernel]::ReadFileCalls -ne 0) -or (-not $watched -and [BridgeVM.BvClipWatch]::ReadCalls -ne 0)) {
            throw "Read-Line $($case[0]): wrong reader"
        }
    }
    Write-Output 'PASS: Read-Line writes CLIPCHANGED and OPENURL between replies only for the watches that are on'
    return
}

# 1. The handler, run as written by the app with a stub Start-Process and a private LOCALAPPDATA.
$swift = [IO.File]::ReadAllText((Join-Path $root 'apps/macos/Sources/BridgeVMControl/HvfEngine/HvfGuestLinks.swift')) -split "`n"
$first = [array]::FindIndex($swift, [Predicate[string]] { param($l) $l.Trim() -ceq 'static let handlerScript = #"""' })
$last = [array]::FindIndex($swift, [int]($first + 1), [Predicate[string]] { param($l) $l -ceq '"""#' })
if ($first -lt 0 -or $last -lt 0) { throw 'missing handler script' }
$handlerText = ($swift[($first + 1)..($last - 1)] -join "`n") + "`n"
if ($handlerText -match '(?i)invoke-expression|\biex\b|-command\b|\bcmd(\.exe)?\b|\bparam\s*\(|-EncodedCommand') { throw 'handler must not run its argument as code' }
if ($handlerText -match '[^\x00-\x7f]') { throw 'handler must be ASCII' }
$work = Join-Path ([IO.Path]::GetTempPath()) ('bv-links-' + [guid]::NewGuid())
[void][IO.Directory]::CreateDirectory($work)
try {
    $handler = Join-Path $work 'bv-open-url.ps1'
    [IO.File]::WriteAllText($handler, $handlerText)
    $env:LOCALAPPDATA = Join-Path $work 'local'
    $links = Join-Path $env:LOCALAPPDATA 'BridgeVM/open-links.txt'
    # The stubs run inside the handler's own script scope: they reach these through the call stack.
    $started = New-Object System.Collections.ArrayList
    $calls = @{ calc = 0 }
    function Start-Process([string]$FilePath) { [void]$started.Add($FilePath) }
    function calc { $calls.calc++ }
    function Get-Links { if (Test-Path -LiteralPath $links) { @(([IO.File]::ReadAllText($links) -split "`r`n") | Where-Object { $_ -ne '' }) } else { @() } }
    function B64([string]$text) { [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($text)) }
    function Invoke-Handler { $started.Clear(); & $handler @args; return $LASTEXITCODE }

    $expected = New-Object System.Collections.ArrayList
    $long = 'https://example.com/' + ('a' * (2048 - 20))
    $korean = 'https://' + [char]0xC608 + [char]0xC2DC + '.kr/' + [char]0xACBD + [char]0xB85C
    foreach ($url in @('https://example.com/?a=1&b=%22x%22', 'HTTP://Example.COM/Path#x', $korean, $long,
        'https://x/$(calc)', 'https://x/;calc', 'https://x/&calc&')) {
        $code = Invoke-Handler $url
        [void]$expected.Add((B64 $url))
        if ($code -ne 0 -or $started.Count -ne 0) { throw "link not queued: $url" }
        if ((@(Get-Links) -join ',') -cne ($expected -join ',')) { throw "link file mismatch after: $url" }
    }
    if ($calls.calc -ne 0) { throw 'a link ran as code' }
    $refused = @(@('https://x/"&calc&"'), @("https://x/ calc"), @("https://x/`tcalc"), @("https://x/`n"), @("https://x/$([char]1)"),
        @("https://x/$([char]0x7f)"), @('javascript:alert(1)'), @('file:///C:/Windows/win.ini'), @('ftp://example.com/'),
        @('microsoft-edge:https://x/'), @('https://'), @('example.com'), @($long + 'a'), @(), @('https://a/', 'https://b/'),
        @('https://x/', '-Command', 'calc'))
    foreach ($argv in $refused) {
        $code = Invoke-Handler @argv
        if ($code -ne 2 -or $started.Count -ne 0 -or (@(Get-Links).Count -ne $expected.Count)) { throw "handler accepted: $($argv -join ' ')" }
    }
    if ($calls.calc -ne 0) { throw 'a refused link ran as code' }
    # This machine's own address opens in Edge here, as an argument, and never reaches the link file.
    foreach ($url in @('http://localhost:8080/x?a=1&b=2', 'HTTP://LOCALHOST/', 'http://127.0.0.1/', 'http://[::1]:3000/', 'http://app.localhost/')) {
        $code = Invoke-Handler $url
        if ($code -ne 0 -or $started.Count -ne 1 -or $started[0] -cne ('microsoft-edge:' + $url) -or @(Get-Links).Count -ne $expected.Count) {
            throw "loopback link mishandled: $url"
        }
    }
    # Past 64 KB the handler starts the file again with the new link.
    [IO.File]::WriteAllText($links, ('A' * 70000) + "`r`n")
    if ((Invoke-Handler 'https://example.com/again') -ne 0 -or (@(Get-Links) -join ',') -cne (B64 'https://example.com/again')) { throw 'link file not started again' }

    # As Windows runs it: powershell.exe -File "<handler>" "%1". A quote in %1 changes the arguments.
    $exe = (Get-Process -Id $PID).Path
    function Invoke-Real([string]$tail) {
        $info = New-Object System.Diagnostics.ProcessStartInfo $exe
        $info.Arguments = '-NoProfile -NonInteractive -ExecutionPolicy Bypass -File "' + $handler + '" ' + $tail
        $info.UseShellExecute = $false
        $process = [System.Diagnostics.Process]::Start($info)
        $process.WaitForExit()
        return $process.ExitCode
    }
    [IO.File]::WriteAllText($links, '')
    if ((Invoke-Real '"https://example.com/?a=1&b=%22x%22"') -ne 0 -or (@(Get-Links) -join ',') -cne (B64 'https://example.com/?a=1&b=%22x%22')) {
        throw 'real invocation did not queue the link'
    }
    $realRefused = @('"https://x/" "-Command" "calc"', '"https://x/ calc"')
    if ($onWindows) { $realRefused += '"https://x/"&calc&""' }
    foreach ($tail in $realRefused) {
        if ((Invoke-Real $tail) -ne 2 -or @(Get-Links).Count -ne 1) { throw "real invocation accepted: $tail" }
    }
    $quoteNote = if ($onWindows) { 'quote split refused' } else { 'quote split checked on Windows only' }
    Write-Output "PASS: handler queues 7 links as base64 data, refuses 16 forms, opens 5 loopback links in Edge, starts its file again past 64 KB, real -File invocation ($quoteNote)"

    # 2. The agent's reader, as compiled in bvagent.ps1, on a file the user owns.
    $assign = $ast.Find({ param($n) $n -is [System.Management.Automation.Language.AssignmentStatementAst] -and $n.Left.Extent.Text -eq '$script:BvClipWatchSource' }, $true)
    Add-Type -TypeDefinition $assign.Right.Expression.Value
    $agentDir = Join-Path $work 'agent/BridgeVM'
    $file = Join-Path $agentDir 'open-links.txt'
    [void][IO.Directory]::CreateDirectory($agentDir)
    function Add-Text([string]$text) { [IO.File]::AppendAllText($file, $text) }
    function Take { @([BridgeVM.BvClipWatch]::TakeLinks()) }
    function Expect-Take([string[]]$want, [string]$what) {
        $got = Take
        if (($got -join ',') -cne ($want -join ',')) { throw "TakeLinks $what`: got '$($got -join ',')'" }
    }
    $a = B64 'https://a.example/'; $b = B64 'https://b.example/?q=1&r=%22'
    Add-Text ($a + "`r`n")
    [BridgeVM.BvClipWatch]::StartLinks($file)
    Expect-Take @() 'lines already there (an earlier agent)'
    Add-Text ($b + "`r`n")
    Expect-Take @($b) 'one new link'
    Add-Text ((@(1..6 | ForEach-Object { B64 "https://n$_.example/" }) -join "`r`n") + "`r`n")
    Expect-Take @(1..4 | ForEach-Object { B64 "https://n$_.example/" }) 'first wake'
    Expect-Take @(5..6 | ForEach-Object { B64 "https://n$_.example/" }) 'second wake'
    Expect-Take @() 'nothing new'
    Add-Text $a
    Expect-Take @() 'a line without its end'
    Add-Text "`r`n"
    Expect-Take @($a) 'the line completed'
    Add-Text ((@('not base64!', (B64 'javascript:alert(1)'), (B64 ('https://x/' + 'a' * 2040)), (B64 'https://x/a b'), (B64 "https://x/$([char]1)"), $b) -join "`r`n") + "`r`n")
    Expect-Take @($b) 'only links pass'
    Add-Text ('A' * 20000)
    Expect-Take @() 'an endless line, first wake'
    Expect-Take @() 'an endless line, second wake'
    Add-Text ("`r`n" + $a + "`r`n")
    Expect-Take @($a) 'after the endless line'
    [IO.File]::WriteAllText($file, $b + "`r`n")
    Expect-Take @($b) 'the file started again'
    Add-Text (($a + "`r`n") * 3000)
    Expect-Take @() 'a 64 KB backlog is skipped'
    Add-Text ($b + "`r`n")
    Expect-Take @($b) 'after the backlog'
    $before = [IO.File]::ReadAllBytes($file)
    # A reparse point in place of the file or its folder is never read: another file's lines stay in the guest.
    $secret = Join-Path $work 'secret.txt'
    [IO.File]::WriteAllText($secret, ($a + "`r`n") * 3)
    [IO.File]::Move($file, $file + '.real')
    [void](New-Item -ItemType SymbolicLink -Path $file -Value $secret)
    Add-Content -LiteralPath $secret -Value $b
    Expect-Take @() 'the file is a symbolic link'
    [IO.File]::Delete($file)
    [IO.File]::Move($file + '.real', $file)
    $other = Join-Path $work 'other'
    [void][IO.Directory]::CreateDirectory($other)
    [IO.File]::WriteAllText((Join-Path $other 'open-links.txt'), (($a + "`r`n") * 5) + ($b + "`r`n"))
    [IO.Directory]::Move($agentDir, $agentDir + '.real')
    if ($onWindows) { [void](New-Item -ItemType Junction -Path $agentDir -Value $other) } else { [void](New-Item -ItemType SymbolicLink -Path $agentDir -Value $other) }
    Expect-Take @() 'the folder is a junction'
    [IO.Directory]::Delete($agentDir)
    [IO.Directory]::Move($agentDir + '.real', $agentDir)
    $after = [IO.File]::ReadAllBytes($file)
    if ([Convert]::ToBase64String($before) -cne [Convert]::ToBase64String($after)) { throw 'the agent changed the link file' }
    Write-Output 'PASS: agent reads only new links, 4 per wake, skips partial, invalid, endless and backlog lines, follows a restarted file, never reads through a reparse point and never changes the file'

    # 3. LINKWATCH starts it (Start-LinkWatch) and the dispatch answers as a command result.
    function Log([string]$text) { }
    . ([scriptblock]::Create((Get-AgentFunction 'Start-LinkWatch')))
    $result = Start-LinkWatch
    if ($result.Exit -ne 0 -or $result.Out -cne 'BVLINKS watch=on' -or -not $script:BvLinkWatch) { throw 'LINKWATCH did not start the watch' }
    if (-not $ast.Extent.Text.Contains("'LINKWATCH' { Write-CommandResult `$h (Start-LinkWatch) }")) { throw 'LINKWATCH dispatch missing' }
} finally {
    Remove-Item -LiteralPath $work -Recurse -Force -ErrorAction SilentlyContinue
}

& (Get-Process -Id $PID).Path -NoProfile -File $PSCommandPath -Mode readline -Agent $Agent
if ($LASTEXITCODE -ne 0) { throw 'Read-Line dispatch failed' }
Write-Output 'PASS: Windows link handler, agent link-file reader and OPENURL dispatch (no registry, no browser)'
