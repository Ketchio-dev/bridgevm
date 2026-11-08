# The guest agent's PUTDATA (a shared-folder chunk whose payload is its own line, decoded as it
# arrives like a Finder drop's): PUTBEG, PUTDATA chunks and PUTEND write the exact file. A chunk
# with the wrong sequence number or a bad payload is refused only after its payload line is read,
# so the next request still parses. Without the compiled reader the same bytes land. Runs the
# agent's own verb cases on an anonymous pipe.
$ErrorActionPreference = 'Stop'
$repo = Split-Path (Split-Path $PSScriptRoot -Parent) -Parent
$source = [IO.File]::ReadAllText((Join-Path $repo 'scripts/win-assets/bvagent.ps1'))
$reader = [regex]::Match($source, "(?s)\`$script:BvDropReaderSource = @'\r?\n(?<cs>.*?)\r?\n'@")
if (-not $reader.Success) { throw 'BvDropReaderSource not found' }
$script:BvDropReaderSource = $reader.Groups['cs'].Value
foreach ($name in @('Is-BadHandle', 'Test-DropReader', 'Read-DataPayload', 'Read-Line', 'Get-IoPath', 'Set-PutTime', 'Close-PutStream')) {
    $function = [regex]::Match($source, "(?s)\nfunction $name\b.*?\r?\n\}\r?\n")
    if (-not $function.Success) { throw "$name not found" }
    . ([scriptblock]::Create($function.Value))
}
$verbs = @{}
foreach ($verb in @('PUTBEG', 'PUTDATA', 'PUTEND')) {
    $one = [regex]::Matches($source, "(?s)\n                '$verb' \{(?<body>.*?)\r?\n                \}\r?\n")
    if ($one.Count -ne 1) { throw "expected exactly one $verb implementation" }
    $verbs[$verb] = [scriptblock]::Create($one[0].Groups['body'].Value)
}
if ($PSVersionTable.PSEdition -eq 'Core') {
    # Windows PowerShell 5.1 compiles C# 5; prove the reader does too.
    Add-Type -TypeDefinition $script:BvDropReaderSource -CompilerOptions '/langversion:5'
    $script:BvDropReader = $true
}
Add-Type -TypeDefinition @'
using System.IO;
using System.Threading;
public static class BvTestPipeFeed {
    public static Thread Start(Stream stream, byte[] data, int piece) {
        Thread thread = new Thread(() => {
            for (int at = 0; at < data.Length; at += piece) {
                stream.Write(data, at, System.Math.Min(piece, data.Length - at));
                stream.Flush();
            }
        });
        thread.IsBackground = true;
        thread.Start();
        return thread;
    }
}
'@
$K = Add-Type -MemberDefinition '[DllImport("kernel32.dll", SetLastError=true)] public static extern bool ReadFile(IntPtr h, byte[] buf, uint toRead, out uint read, IntPtr overlapped);' -Name 'BvTestKernel32' -Namespace 'BridgeVM' -PassThru
function Log([string]$m) { }
function Write-Line($handle, [string]$line, [string]$label) { $script:sent = $line }
function Assert($condition, [string]$what) { if (-not $condition) { throw "FAIL: $what" } }

# Run requests (header line, then for PUTDATA its payload line on the pipe) through the verb
# blocks as the main loop does, and collect the replies.
function Run-Requests($requests) {
    $script:BvReadBuffer = New-Object byte[] 16384
    $script:BvPending = New-Object System.Text.StringBuilder
    $script:BvLines = New-Object 'System.Collections.Generic.Queue[string]'
    $script:BvClipPush = $false
    $server = New-Object System.IO.Pipes.AnonymousPipeServerStream ([IO.Pipes.PipeDirection]::Out)
    $client = New-Object System.IO.Pipes.AnonymousPipeClientStream -ArgumentList ([IO.Pipes.PipeDirection]::In), $server.ClientSafePipeHandle
    try {
        $wire = New-Object System.Text.StringBuilder
        foreach ($r in $requests) { if ($r.Payload -ne $null) { [void]$wire.Append($r.Payload + "`n") } }
        $feed = [BvTestPipeFeed]::Start($server, [Text.Encoding]::ASCII.GetBytes($wire.ToString()), 4096)
        $h = $client.SafePipeHandle.DangerousGetHandle()
        $replies = foreach ($r in $requests) {
            $arg = $r.Arg
            & $verbs[$r.Verb]
            $script:sent
        }
        [void]$feed.Join(5000)
        return @($replies)
    } finally {
        $client.Dispose(); $server.Dispose()
    }
}
function B64([string]$text) { [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($text)) }
function Data([int]$seq, [byte[]]$bytes) {
    $payload = [Convert]::ToBase64String($bytes)
    @{ Verb = 'PUTDATA'; Arg = "$seq $($payload.Length)"; Payload = $payload }
}

$root = Join-Path ([IO.Path]::GetTempPath()) ('bv-putdata-' + [Guid]::NewGuid().ToString('N'))
[void][IO.Directory]::CreateDirectory($root)
$cases = 0
try {
    $rng = New-Object Random 13
    $one = New-Object byte[] (1024 * 1024); $rng.NextBytes($one)
    $two = New-Object byte[] (1024 * 1024); $rng.NextBytes($two)
    $tail = [byte[]](1, 2, 3, 4, 5)
    $all = New-Object IO.MemoryStream
    foreach ($part in @($one, $two, $tail)) { $all.Write($part, 0, $part.Length) }
    $want = $all.ToArray()
    foreach ($compiled in @($true, $false)) {
        $script:BvDropReader = if ($compiled) { $null } else { $false }
        if ($compiled -and $PSVersionTable.PSEdition -eq 'Core') { $script:BvDropReader = $true }
        $path = Join-Path $root "file-$compiled.bin"
        $replies = Run-Requests @(
            @{ Verb = 'PUTBEG'; Arg = (B64 $path) + " $($want.Length) 3"; Payload = $null },
            (Data 0 $one), (Data 1 $two), (Data 2 $tail),
            @{ Verb = 'PUTEND'; Arg = '3'; Payload = $null })
        Assert ($replies[0] -ceq 'OK PUTBEG') "PUTBEG: $($replies[0])"
        Assert (($replies[1..3] -join ',') -ceq 'OK PUTCHUNK 0,OK PUTCHUNK 1,OK PUTCHUNK 2') "chunks: $($replies[1..3] -join ',')"
        Assert ($replies[4] -clike 'PUTOK *') "PUTEND: $($replies[4])"
        $got = [IO.File]::ReadAllBytes($path)
        Assert ([Convert]::ToBase64String($got) -ceq [Convert]::ToBase64String($want)) "compiled=${compiled}: the file differs ($($got.Length) bytes)"
        $cases++
    }
    Assert ($script:BvDropReader -eq $false) 'the fallback run did not use the compiled reader'

    # A wrong sequence number, then a payload with a bad character: each refused after its line,
    # and the chunk after them still lands.
    $script:BvDropReader = $null
    if ($PSVersionTable.PSEdition -eq 'Core') { $script:BvDropReader = $true }
    $path = Join-Path $root 'refused.bin'
    $bad = 'AAA*'
    $replies = Run-Requests @(
        @{ Verb = 'PUTBEG'; Arg = (B64 $path) + ' 3 1'; Payload = $null },
        (Data 5 ([byte[]](9, 9, 9))),
        @{ Verb = 'PUTDATA'; Arg = "0 $($bad.Length)"; Payload = $bad },
        (Data 0 ([byte[]](7, 8, 9))),
        @{ Verb = 'PUTEND'; Arg = '1'; Payload = $null })
    Assert ($replies[1] -ceq 'ERR PUTCHUNK seq') "wrong sequence: $($replies[1])"
    Assert ($replies[2] -clike 'ERR PUTCHUNK *') "bad payload: $($replies[2])"
    Assert ($replies[3] -ceq 'OK PUTCHUNK 0') "the chunk after them: $($replies[3])"
    Assert ($replies[4] -clike 'PUTOK *' -and (([IO.File]::ReadAllBytes($path)) -join ',') -ceq '7,8,9') "after refusals: $($replies[4])"
    $cases++
} finally {
    Close-PutStream
    Remove-Item -LiteralPath $root -Recurse -Force -ErrorAction SilentlyContinue
}
Write-Output "agent put data contract: PASS ($cases cases, $($PSVersionTable.PSEdition))"
