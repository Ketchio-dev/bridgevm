# The guest agent's large shared-folder verbs. GETRANGE returns the asked bytes of a file with its
# size and time after the read, in 768 KiB chunks, and a range past the end is empty, so the host
# can reassemble a file range by range and tell when it changed meanwhile. LPUTBEG/LPUTDATA/LPUTEND
# write a temporary and rename it over the target only whole, with the Mac's time; an abort, a bad
# chunk or a short end deletes the temporary and leaves the target as it was; a volume without the
# room answers disk-full; LSR never lists a temporary (exactly .bridgevm-part-<32 hex>) and deletes
# one no PUT holds once it is a few minutes old, while a user's look-alike name is a file like any. Runs the
# agent's own verb cases against a temporary folder (no serial port: replies are collected).
$ErrorActionPreference = 'Stop'
$repo = Split-Path (Split-Path $PSScriptRoot -Parent) -Parent
$source = [IO.File]::ReadAllText((Join-Path $repo 'scripts/win-assets/bvagent.ps1'))
$verbs = @{}
foreach ($verb in @('GETRANGE', 'LPUTBEG', 'LPUTDATA', 'LPUTEND', 'LPUTABORT', 'LSR')) {
    $one = [regex]::Matches($source, "(?s)\n                '$verb' \{(?<body>.*?)\r?\n                \}\r?\n")
    if ($one.Count -ne 1) { throw "expected exactly one $verb implementation" }
    $body = $one[0].Groups['body'].Value
    if ($verb -eq 'LSR' -and [IO.Path]::DirectorySeparatorChar -ne '\') {
        # LSR roots its listing at <root>\, which only Windows paths match: elsewhere (pwsh on
        # macOS) the test gives it the host's separator, or it lists nothing.
        $windowsRoot = "if (-not `$rootFull.EndsWith('\')) { `$rootFull = `$rootFull + '\' }"
        if (-not $body.Contains($windowsRoot)) { throw 'LSR root separator not found' }
        $body = $body.Replace($windowsRoot, $windowsRoot.Replace("'\'", "'/'"))
    }
    $verbs[$verb] = [scriptblock]::Create($body)
}
foreach ($name in @('Get-IoPath', 'Set-PutTime', 'Is-BadHandle', 'Test-DropReader', 'Read-DataPayload')) {
    $function = [regex]::Match($source, "(?s)\nfunction $name\b.*?\r?\n\}\r?\n")
    if (-not $function.Success) { throw "$name not found" }
    . ([scriptblock]::Create($function.Value))
}
foreach ($block in @('large put', 'share watch')) {
    $found = [regex]::Match($source, "(?s)# >>> $block.*?# <<< $block")
    if (-not $found.Success) { throw "$block block not found" }
    . ([scriptblock]::Create($found.Value))
}
$reader = [regex]::Match($source, "(?s)\`$script:BvDropReaderSource = @'\r?\n(?<cs>.*?)\r?\n'@")
if (-not $reader.Success) { throw 'BvDropReaderSource not found' }
$script:BvDropReaderSource = $reader.Groups['cs'].Value
if ($PSVersionTable.PSEdition -eq 'Core') {
    # Windows PowerShell 5.1 compiles C# 5; prove the reader (with its MoveFileEx) does too.
    Add-Type -TypeDefinition $script:BvDropReaderSource -CompilerOptions '/langversion:5'
    $script:BvDropReader = $true
}
$script:BvLines = New-Object 'System.Collections.Generic.Queue[string]'
$script:BvPending = New-Object System.Text.StringBuilder
function Assert($condition, [string]$what) { if (-not $condition) { throw "FAIL: $what" } }
function Log([string]$m) { }
# Quick Access has its own Shell-mocked contract; LSR extraction needs its entry point too.
function Pin-ShareQuickAccess([string]$root) { }
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

    # LPUT: run requests through the verbs; a payload goes ahead in the line queue, as Read-Line left it.
    function Put([string]$verb, [string]$argument, [byte[]]$payload) {
        $script:sent = New-Object System.Collections.ArrayList
        if ($null -ne $payload) { $script:BvLines.Enqueue([Convert]::ToBase64String($payload)) }
        $arg = $argument
        & $verbs[$verb]
        return $script:sent[0]
    }
    function Parts([string]$dir) { @([IO.Directory]::GetFiles($dir) | ForEach-Object { [IO.Path]::GetFileName($_) } | Where-Object { $_ -cmatch '^\.bridgevm-part-[0-9a-f]{32}$' }) }
    $target = [IO.Path]::Combine($root, 'sub', 'movie.mov')
    [void][IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($target))
    [IO.File]::WriteAllBytes($target, [byte[]](9, 9))
    $one = New-Object byte[] 1048576; (New-Object Random 3).NextBytes($one)
    $tail = [byte[]](1, 2, 3)
    $macTime = [DateTime]::new(2026, 11, 8, 0, 5, 12, 123, [DateTimeKind]::Utc)
    Assert ((Put 'LPUTBEG' ((B64 $target) + ' 1048579 1794096312123') $null) -ceq 'OK LPUTBEG') 'LPUTBEG'
    Assert ((Put 'LPUTDATA' "0 $([Convert]::ToBase64String($one).Length)" $one) -ceq 'OK LPUTDATA 0') 'LPUTDATA 0'
    Assert (((Parts ([IO.Path]::GetDirectoryName($target))).Count -eq 1) -and ([IO.File]::ReadAllBytes($target).Length -eq 2)) 'mid-file: the old target stays, the new bytes are in a temporary'
    Assert ((Put 'LPUTDATA' '1 4' $tail) -ceq 'OK LPUTDATA 1') 'LPUTDATA 1'
    $reply = Put 'LPUTEND' '2' $null
    Assert ($reply -ceq ('PUTOK ' + (B64 $target) + ' 1048579')) "LPUTEND: $reply"
    $got = [IO.File]::ReadAllBytes($target)
    Assert ($got.Length -eq 1048579 -and [Convert]::ToBase64String($got, 0, 1048576) -ceq [Convert]::ToBase64String($one)) 'the target holds the new file'
    Assert ([IO.File]::GetLastWriteTimeUtc($target) -eq $macTime) "the Mac's time: $([IO.File]::GetLastWriteTimeUtc($target).ToString('o'))"
    Assert ((Parts ([IO.Path]::GetDirectoryName($target))).Count -eq 0) 'no temporary after LPUTEND'
    $cases++

    # Abort, a wrong sequence number, a short end, more than announced: the target as it was, no temporary.
    $dir = [IO.Path]::GetDirectoryName($target)
    foreach ($bad in @(
            @{ Steps = { Put 'LPUTABORT' '' $null }; Want = 'OK LPUTABORT' },
            @{ Steps = { Put 'LPUTDATA' '5 4' $tail }; Want = 'ERR LPUTDATA seq' },
            @{ Steps = { Put 'LPUTEND' '0' $null }; Want = 'ERR LPUTEND short' },
            @{ Steps = { Put 'LPUTDATA' "0 $([Convert]::ToBase64String($one).Length)" $one }; Want = 'ERR LPUTDATA past-total' })) {
        Assert ((Put 'LPUTBEG' ((B64 $target) + ' 3') $null) -ceq 'OK LPUTBEG') 'LPUTBEG again'
        $reply = & $bad.Steps
        Assert ($reply -ceq $bad.Want) "refusal: $reply"
        Assert ((Parts $dir).Count -eq 0 -and $null -eq $script:BvLPut) "after '$reply' a temporary or stream is left"
        Assert ([IO.File]::ReadAllBytes($target).Length -eq 1048579) "after '$reply' the target changed"
        $cases++
    }
    Assert ((Put 'LPUTDATA' '0 4' $tail) -ceq 'ERR LPUTDATA no-open-put') 'a chunk with no stream'
    Assert ($script:BvLines.Count -eq 0) 'its payload line was read'
    $cases++

    # More than the volume holds: disk-full, nothing created.
    Assert ((Put 'LPUTBEG' ((B64 $target) + ' ' + [int64]::MaxValue) $null) -ceq 'ERR LPUTBEG disk-full') 'disk-full at LPUTBEG'
    Assert ((Parts $dir).Count -eq 0) 'a temporary after disk-full'
    $full = @{ Exception = (New-Object IO.IOException 'There is not enough space on the disk.', -2147024784) }
    Assert ((Get-PutError $full) -ceq 'disk-full') 'ERROR_DISK_FULL is disk-full'
    $cases++

    # LSR: the open stream's temporary is neither listed nor deleted; an old leftover is deleted and a
    # recent one kept, both unlisted; a user's file whose name only starts like one is listed and kept.
    $old = [IO.Path]::Combine($dir, '.bridgevm-part-' + [Guid]::NewGuid().ToString('N'))
    $recent = [IO.Path]::Combine($dir, '.bridgevm-part-' + [Guid]::NewGuid().ToString('N'))
    $users = @('.bridgevm-part-notes', ('.bridgevm-part-' + [Guid]::NewGuid().ToString('N').ToUpperInvariant()))
    foreach ($file in @($old, $recent) + @($users | ForEach-Object { [IO.Path]::Combine($dir, $_) })) { [IO.File]::WriteAllBytes($file, [byte[]](1)) }
    [IO.File]::SetLastWriteTimeUtc($old, [DateTime]::UtcNow.AddMinutes(-10))
    Assert ((Put 'LPUTBEG' ((B64 $target) + ' 3') $null) -ceq 'OK LPUTBEG') 'LPUTBEG for LSR'
    $script:BvLsrChanges = [long]-1
    $reply = Put 'LSR' (B64 $root) $null

    $listing = [Text.Encoding]::UTF8.GetString([Convert]::FromBase64String($reply.Substring(5)))
    $listed = @($listing -split "`r?`n" | Where-Object { $_ } | ForEach-Object { [IO.Path]::GetFileName(($_ -split '\|')[0].Replace('\', '/')) } | Where-Object { $_.StartsWith('.bridgevm-part-', [StringComparison]::OrdinalIgnoreCase) })
    Assert ($reply -clike 'LSOK *' -and $listing -cnotmatch 'bridgevm-part-[0-9a-f]{32}\|') "listed a temporary: $listing"
    Assert ((($listed | Sort-Object) -join ',') -ceq (($users | Sort-Object) -join ',')) "listed: $($listed -join ',')"
    $left = @(Parts $dir | Sort-Object)
    $want = @([IO.Path]::GetFileName($script:BvLPut.Temp), [IO.Path]::GetFileName($recent)) | Sort-Object
    Assert (($left -join ',') -ceq ($want -join ',')) "temporaries left: $($left -join ',')"
    foreach ($user in $users) { Assert ([IO.File]::Exists([IO.Path]::Combine($dir, $user))) "deleted the user's $user" }
    Assert ((Put 'LPUTABORT' '' $null) -ceq 'OK LPUTABORT') 'abort for LSR'
    $cases++

    # A file another program holds open for writing can still be read.
    $held = [IO.File]::Open($path, 'Open', 'ReadWrite', 'ReadWrite')
    try { Assert ((Get-Range $path 0 4).Bytes -eq 4) 'a file open for writing is read' } finally { $held.Close() }
    $cases++
} finally {
    Close-LargePut
    Remove-Item -LiteralPath $root -Recurse -Force -ErrorAction SilentlyContinue
}
Write-Output "agent share large contract: PASS ($cases cases, $($PSVersionTable.PSEdition))"
