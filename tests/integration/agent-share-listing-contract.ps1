# The guest agent's shared-folder listing (LSR) with its change watcher (BridgeVM.BvShareWatch):
# an unchanged folder gets the last listing again without being enumerated, and any change
# below the root (a new, grown, deleted or renamed file, a new folder, a change made while the
# folder was being listed) gets a fresh, correct listing. Runs the agent's own LSR case and
# share watch block against a temporary folder.
$ErrorActionPreference = 'Stop'
$repo = Split-Path (Split-Path $PSScriptRoot -Parent) -Parent
$source = [IO.File]::ReadAllText((Join-Path $repo 'scripts/win-assets/bvagent.ps1'))
$block = [regex]::Match($source, '(?s)# >>> share watch.*?# <<< share watch')
if (-not $block.Success) { throw 'share watch block not found' }
$found = [regex]::Matches($source, "(?s)\n                'LSR' \{(?<body>.*?)\r?\n                \}\r?\n")
if ($found.Count -ne 1) { throw 'expected exactly one LSR implementation' }
$lsr = [scriptblock]::Create($found[0].Groups['body'].Value)
$verbs = @{}
foreach ($verb in @('PUT', 'DEL')) {
    $one = [regex]::Matches($source, "(?s)\n                '$verb' \{(?<body>.*?)\r?\n                \}\r?\n")
    if ($one.Count -ne 1) { throw "expected exactly one $verb implementation" }
    $verbs[$verb] = [scriptblock]::Create($one[0].Groups['body'].Value)
}
. ([scriptblock]::Create($block.Value))
if ($PSVersionTable.PSEdition -eq 'Core') {
    # Windows PowerShell 5.1 compiles C# 5; prove the helper does too.
    Add-Type -TypeDefinition $script:BvShareWatchSource -CompilerOptions '/langversion:5' -ErrorAction Stop
    $script:BvShareWatchReady = $true
}

function Assert($condition, [string]$what) { if (-not $condition) { throw "FAIL: $what" } }
function Log([string]$m) { }
function Write-Line($handle, [string]$line, [string]$label) { $script:sent = $line }
$script:enumerations = 0
$script:during = $null
function Get-ChildItem {
    $script:enumerations++
    $items = @(Microsoft.PowerShell.Management\Get-ChildItem @args)
    if ($script:during) { & $script:during; $script:during = $null }
    $items
}
$h = [IntPtr]::Zero
$root = Join-Path ([IO.Path]::GetTempPath()) ('bv-lsr-' + [Guid]::NewGuid().ToString('N'))
[void][IO.Directory]::CreateDirectory((Join-Path $root 'sub'))

function List {
    $before = $script:enumerations
    $arg = [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($root))
    & $lsr
    Assert ($script:sent -clike 'LSOK *') "LSR reply: $script:sent"
    $text = [Text.Encoding]::UTF8.GetString([Convert]::FromBase64String($script:sent.Substring(5)))
    $entries = @{}
    foreach ($row in ($text -split "`r?`n" | Where-Object { $_ })) {
        $f = $row.Split('|')
        $entries[$f[0].Replace('\', '/')] = $f[1]
    }
    return @{ Enumerated = ($script:enumerations -gt $before); Entries = $entries }
}
# Wait for the watcher to see a change, then for it to be quiet for 500 ms: one change can
# arrive as several events, some late (seen on Windows for files written just before the
# watch began). The host's next poll is 2 s away.
function Settle([long]$from) {
    $clock = [Diagnostics.Stopwatch]::StartNew()
    while ([BridgeVM.BvShareWatch]::Changes -eq $from -and $clock.ElapsedMilliseconds -lt 5000) { Start-Sleep -Milliseconds 50 }
    Quiet
}
function Quiet {
    $clock = [Diagnostics.Stopwatch]::StartNew()
    $seen = [BridgeVM.BvShareWatch]::Changes; $since = 0
    while ($clock.ElapsedMilliseconds - $since -lt 500 -and $clock.ElapsedMilliseconds -lt 10000) {
        Start-Sleep -Milliseconds 50
        $now = [BridgeVM.BvShareWatch]::Changes
        if ($now -ne $seen) { $seen = $now; $since = $clock.ElapsedMilliseconds }
    }
}
function Change([scriptblock]$what) {
    $from = [BridgeVM.BvShareWatch]::Changes
    & $what
    Settle $from
}

$cases = 0
try {
    $name = 'sub/' + [char]0xBCF4 + [char]0xACE0 + 'x.txt'
    [IO.File]::WriteAllBytes((Join-Path $root 'a.txt'), [byte[]](1, 2, 3))
    [IO.File]::WriteAllBytes((Join-Path $root $name), (New-Object byte[] 10))
    $first = List
    Assert $first.Enumerated 'the first listing enumerates'
    Assert ($first.Entries['a.txt'] -eq '3' -and $first.Entries[$name] -eq '10' -and $first.Entries['sub'] -eq '0') "first listing: $($first.Entries.Keys -join ',')"
    $cases++

    Quiet
    [void](List)
    $again = List
    Assert (-not $again.Enumerated) 'an unchanged folder is not enumerated again'
    Assert ($again.Entries.Count -eq $first.Entries.Count) 'and gets the same listing'
    $cases++

    $changes = @(
        @('a new file', { [IO.File]::WriteAllBytes((Join-Path $root 'b.bin'), (New-Object byte[] 7)) }, { param($e) $e['b.bin'] -eq '7' }),
        @('a file that grew', { $s = [IO.File]::Open((Join-Path $root 'a.txt'), 'Append'); $s.Write([byte[]](9, 9), 0, 2); $s.Close() }, { param($e) $e['a.txt'] -eq '5' }),
        @('a deleted file', { [IO.File]::Delete((Join-Path $root 'b.bin')) }, { param($e) -not $e.ContainsKey('b.bin') }),
        @('a renamed file', { [IO.File]::Move((Join-Path $root 'a.txt'), (Join-Path $root 'sub/c.txt')) }, { param($e) -not $e.ContainsKey('a.txt') -and $e['sub/c.txt'] -eq '5' }),
        @('a new folder with a file', { [void][IO.Directory]::CreateDirectory((Join-Path $root 'new/deep')); [IO.File]::WriteAllBytes((Join-Path $root 'new/deep/d'), [byte[]](1)) }, { param($e) $e['new/deep/d'] -eq '1' })
    )
    foreach ($c in $changes) {
        Change $c[1]
        $listing = List
        Assert $listing.Enumerated "$($c[0]) is listed afresh"
        Assert (& $c[2] $listing.Entries) "$($c[0]): $(($listing.Entries.GetEnumerator() | ForEach-Object { $_.Key + '=' + $_.Value }) -join ',')"
        Quiet
        [void](List)
        Assert (-not (List).Enumerated) "after $($c[0]) the folder is cached again"
        $cases++
    }

    # A file written while the folder is being enumerated is missing from that listing, so the
    # next listing must not be the cached one.
    Quiet
    $from = [BridgeVM.BvShareWatch]::Changes
    $script:during = { [IO.File]::WriteAllBytes((Join-Path $root 'late.txt'), [byte[]](4, 4)) }
    $script:BvLsrChanges = [long]-1
    $racing = List
    Assert (-not $racing.Entries.ContainsKey('late.txt')) 'the racing write is not in the listing it raced'
    Settle $from
    $after = List
    Assert ($after.Enumerated -and $after.Entries['late.txt'] -eq '2') 'a write during a listing is listed next time'
    $cases++

    # The agent's own writes (the host's PUT and DEL) are listed by the very next LSR, before
    # the watcher's events for them arrive: the host takes that listing as its write landing.
    $b64 = { param($text) [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($text)) }
    $putPath = Join-Path $root 'put.bin'
    foreach ($bytes in @([byte[]](1, 2, 3), [byte[]](7, 8, 9))) {
        Quiet; [void](List); Assert (-not (List).Enumerated) 'cached before the PUT'
        $arg = (& $b64 $putPath) + ' ' + [Convert]::ToBase64String($bytes)
        & $verbs['PUT']
        Assert ($script:sent -clike 'PUTOK *') "PUT reply: $script:sent"
        $listing = List
        Assert ($listing.Enumerated -and $listing.Entries['put.bin'] -eq '3') 'the LSR right after a PUT lists it'
        $cases++
    }
    Quiet; [void](List); Assert (-not (List).Enumerated) 'cached before the DEL'
    $arg = & $b64 $putPath
    & $verbs['DEL']
    Assert ($script:sent -ceq 'OK DEL') "DEL reply: $script:sent"
    $listing = List
    Assert ($listing.Enumerated -and -not $listing.Entries.ContainsKey('put.bin')) 'the LSR right after a DEL no longer lists the file'
    $cases++

    # Never older than the age limit, whatever the watcher says.
    $script:BvLsrMaxAgeTicks = [long]1
    Start-Sleep -Milliseconds 5
    Assert (List).Enumerated 'a listing past its age is enumerated again'
    $script:BvLsrMaxAgeTicks = [long](60 * [Diagnostics.Stopwatch]::Frequency)
    $cases++

    # Without the watcher every listing enumerates, as before.
    $script:BvShareWatchReady = $false
    Assert ((List).Enumerated -and (List).Enumerated) 'without the watcher every LSR enumerates'
    $cases++
} finally {
    Remove-Item -LiteralPath $root -Recurse -Force -ErrorAction SilentlyContinue
}
Write-Output "agent share listing contract: PASS ($cases cases, $($PSVersionTable.PSEdition))"
