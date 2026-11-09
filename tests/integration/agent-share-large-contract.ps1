# The guest agent's large shared-folder verbs. GETRANGE returns the asked bytes of a file with its
# size and time after the read, in 768 KiB chunks, and a range past the end is empty, so the host
# can reassemble a file range by range and tell when it changed meanwhile. Runs the agent's own
# verb cases against a temporary folder (no serial port: replies are collected).
$ErrorActionPreference = 'Stop'
$repo = Split-Path (Split-Path $PSScriptRoot -Parent) -Parent
$source = [IO.File]::ReadAllText((Join-Path $repo 'scripts/win-assets/bvagent.ps1'))
$verbs = @{}
foreach ($verb in @('GETRANGE')) {
    $one = [regex]::Matches($source, "(?s)\n                '$verb' \{(?<body>.*?)\r?\n                \}\r?\n")
    if ($one.Count -ne 1) { throw "expected exactly one $verb implementation" }
    $verbs[$verb] = [scriptblock]::Create($one[0].Groups['body'].Value)
}
foreach ($name in @('Get-IoPath')) {
    $function = [regex]::Match($source, "(?s)\nfunction $name\b.*?\r?\n\}\r?\n")
    if (-not $function.Success) { throw "$name not found" }
    . ([scriptblock]::Create($function.Value))
}
function Assert($condition, [string]$what) { if (-not $condition) { throw "FAIL: $what" } }
function Log([string]$m) { }
function Write-Line($handle, [string]$line, [string]$label) { [void]$script:sent.Add($line) }
function B64([string]$text) { [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($text)) }
$h = [IntPtr]::Zero

# GETRANGE of `path` from `offset`, `length` long: the header fields and the reassembled bytes.
function Get-Range([string]$path, [long]$offset, [long]$length) {
    $script:sent = New-Object System.Collections.ArrayList
    $arg = (B64 $path) + " $offset $length"
    & $verbs['GETRANGE']
    $head = $script:sent[0].Split(' ')
    if ($head[0] -cne 'GETBEG') { return @{ Error = $script:sent[0] } }
    $data = New-Object IO.MemoryStream
    $chunks = 0
    for ($i = 1; $i -lt $script:sent.Count - 1; $i++) {
        $f = $script:sent[$i].Split(' ')
        Assert ($f[0] -ceq 'GETCHUNK' -and [int]$f[1] -eq $chunks) "chunk line $i"
        $b = [Convert]::FromBase64String($f[2])
        Assert ($b.Length -le 786432) "chunk $i is $($b.Length) bytes"
        $data.Write($b, 0, $b.Length)
        $chunks++
    }
    Assert ($script:sent[$script:sent.Count - 1] -ceq "GETEND $chunks") "GETEND: $($script:sent[$script:sent.Count - 1])"
    return @{ Path = [Text.Encoding]::UTF8.GetString([Convert]::FromBase64String($head[1])); Bytes = [long]$head[2]; Chunks = [int]$head[3]
              Offset = [long]$head[4]; Size = [long]$head[5]; Time = $head[6]; Data = $data.ToArray() }
}

$root = Join-Path ([IO.Path]::GetTempPath()) ('bv-large-' + [Guid]::NewGuid().ToString('N'))
[void][IO.Directory]::CreateDirectory($root)
$cases = 0
try {
    $path = [IO.Path]::Combine($root, 'big ' + [char]0xD55C + '.bin')
    $want = New-Object byte[] (3 * 1024 * 1024 + 5)
    (New-Object Random 7).NextBytes($want)
    [IO.File]::WriteAllBytes($path, $want)
    $time = [IO.File]::GetLastWriteTimeUtc($path).ToString('o')

    # Ranges of 1.25 MiB put the file back together, each with the file's size and time.
    $all = New-Object IO.MemoryStream
    $step = 1310720
    for ($at = [long]0; $at -lt $want.Length; $at += $step) {
        $r = Get-Range $path $at $step
        Assert ($r.Path -ceq $path) "the reply names the file: $($r.Path)"
        Assert ($r.Offset -eq $at -and $r.Size -eq $want.Length -and $r.Time -ceq $time) "range at ${at}: $($r.Offset) $($r.Size) $($r.Time)"
        Assert ($r.Bytes -eq [Math]::Min($step, $want.Length - $at) -and $r.Data.Length -eq $r.Bytes) "range at $at holds $($r.Bytes)/$($r.Data.Length)"
        Assert ($r.Chunks -eq [Math]::Ceiling($r.Bytes / 786432.0)) "range at $at has $($r.Chunks) chunks"
        $all.Write($r.Data, 0, $r.Data.Length)
    }
    Assert ([Convert]::ToBase64String($all.ToArray()) -ceq [Convert]::ToBase64String($want)) 'the ranges reassemble the file'
    $cases++

    # The time is LSR's: the same text the listing gives the file.
    $listed = (Get-Item -LiteralPath $path).LastWriteTimeUtc.ToString('o')
    Assert ((Get-Range $path 0 10).Time -ceq $listed) "GETRANGE time $time vs listed $listed"
    $cases++

    # A file that grew between two ranges reports its new size (the host drops the copy).
    $s = [IO.File]::Open($path, 'Append'); $s.Write([byte[]](1, 2, 3), 0, 3); $s.Close()
    $r = Get-Range $path $step $step
    Assert ($r.Size -eq $want.Length + 3) "grown size: $($r.Size)"
    $cases++

    # Past the end: no bytes, the size tells why.
    $r = Get-Range $path ($want.Length + 100) $step
    Assert ($r.Bytes -eq 0 -and $r.Chunks -eq 0 -and $r.Data.Length -eq 0 -and $r.Size -eq $want.Length + 3) "past the end: $($r.Bytes) $($r.Size)"
    $cases++

    # A missing file is an error line, not an exception out of the loop.
    $r = Get-Range ([IO.Path]::Combine($root, 'missing.bin')) 0 10
    Assert ($r.Error -clike 'ERR GETRANGE *') "missing file: $($r.Error)"
    $cases++

    # A file another program holds open for writing can still be read.
    $held = [IO.File]::Open($path, 'Open', 'ReadWrite', 'ReadWrite')
    try { Assert ((Get-Range $path 0 4).Bytes -eq 4) 'a file open for writing is read' } finally { $held.Close() }
    $cases++
} finally {
    Remove-Item -LiteralPath $root -Recurse -Force -ErrorAction SilentlyContinue
}
Write-Output "agent share large contract: PASS ($cases cases, $($PSVersionTable.PSEdition))"
