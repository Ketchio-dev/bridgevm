# The guest agent's shared-folder listing (LSR) with its change watcher (BridgeVM.BvShareWatch):
# an unchanged folder gets the last listing again without being enumerated, and any change
# below the root (a new, grown, deleted or renamed file, a new folder, a change made while the
# folder was being listed) gets a fresh, correct listing. A host that names the listing's gen
# (READY lssame) gets LSSAME for it while it holds, and the listing with a new gen after any
# change, at the age limit, after a lost watcher event and after its own write; a host without
# a gen gets the reply as before. A file the host writes (PUT, or
# PUTBEG/PUTCHUNK/PUTEND) keeps the Mac file's modification time when the host sends one. Runs
# the agent's own verb cases and share watch block against a temporary folder. On Windows, a
# path longer than 260 characters is written, listed and deleted, and its listing does not fail.
# The compiled listing (BvShareWatch.List) gives exactly the rows the Get-ChildItem walk gave.
$ErrorActionPreference = 'Stop'
$repo = Split-Path (Split-Path $PSScriptRoot -Parent) -Parent
$source = [IO.File]::ReadAllText((Join-Path $repo 'scripts/win-assets/bvagent.ps1'))
$block = [regex]::Match($source, '(?s)# >>> share watch.*?# <<< share watch')
if (-not $block.Success) { throw 'share watch block not found' }
$found = [regex]::Matches($source, "(?s)\n                'LSR' \{(?<body>.*?)\r?\n                \}\r?\n")
if ($found.Count -ne 1) { throw 'expected exactly one LSR implementation' }
$body = $found[0].Groups['body'].Value
if ([IO.Path]::DirectorySeparatorChar -ne '\') {
    # LSR roots its listing at <root>\, which only Windows paths match: elsewhere (pwsh on
    # macOS) the test gives it the host's separator, or it lists nothing.
    $windowsRoot = "if (-not `$rootFull.EndsWith('\')) { `$rootFull = `$rootFull + '\' }"
    if (-not $body.Contains($windowsRoot)) { throw 'LSR root separator not found' }
    $body = $body.Replace($windowsRoot, $windowsRoot.Replace("'\'", "'/'"))
}
$lsr = [scriptblock]::Create($body)
$verbs = @{}
foreach ($verb in @('PUT', 'DEL', 'PUTBEG', 'PUTCHUNK', 'PUTEND')) {
    $one = [regex]::Matches($source, "(?s)\n                '$verb' \{(?<body>.*?)\r?\n                \}\r?\n")
    if ($one.Count -ne 1) { throw "expected exactly one $verb implementation" }
    $verbs[$verb] = [scriptblock]::Create($one[0].Groups['body'].Value)
}
. ([scriptblock]::Create($block.Value))
foreach ($name in @('Get-IoPath', 'Set-PutTime', 'Close-PutStream')) {
    $function = [regex]::Match($source, "(?s)\nfunction $name\b.*?\r?\n\}\r?\n")
    if (-not $function.Success) { throw "$name not found" }
    . ([scriptblock]::Create($function.Value))
}
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
$listShare = ${function:Get-ShareListing}
function Get-ShareListing([string]$listRoot) {
    $script:enumerations++
    $rows = & $listShare $listRoot
    if ($script:during) { & $script:during; $script:during = $null }
    $rows
}
$maxAge = [long](600 * [Diagnostics.Stopwatch]::Frequency)
if ($script:BvLsrMaxAgeTicks -ne $maxAge) { throw 'FAIL: the listing is re-sent for ten minutes at most' }
$h = [IntPtr]::Zero
$root = Join-Path ([IO.Path]::GetTempPath()) ('bv-lsr-' + [Guid]::NewGuid().ToString('N'))
[void][IO.Directory]::CreateDirectory((Join-Path $root 'sub'))

# LSR, naming `gen` when given: Same is LSSAME's gen, Gen the listing's.
function List([string]$gen) {
    $before = $script:enumerations
    $arg = [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($root))
    if ($gen) { $arg += ' ' + $gen }
    & $lsr
    $reply = @{ Enumerated = ($script:enumerations -gt $before); Same = $null; Gen = $null; Entries = @{} }
    if ($gen -and $script:sent -clike 'LSSAME *') {
        $reply.Same = $script:sent.Substring(7)
        return $reply
    }
    Assert ($script:sent -clike 'LSOK *') "LSR reply: $script:sent"
    $fields = $script:sent.Substring(5).Split(' ')
    if ($gen) {
        Assert ($fields.Count -eq 2 -and $fields[1] -cmatch '^[0-9a-f]{32}$') "LSOK with its gen: $($script:sent.Substring(0, [Math]::Min(80, $script:sent.Length)))"
        $reply.Gen = $fields[1]
    } else {
        Assert ($fields.Count -eq 1) 'LSR without a gen: the reply as before'
    }
    $text = [Text.Encoding]::UTF8.GetString([Convert]::FromBase64String($fields[0]))
    foreach ($row in ($text -split "`r?`n" | Where-Object { $_ })) {
        $f = $row.Split('|')
        $reply.Entries[$f[0].Replace('\', '/')] = $f[1]
    }
    return $reply
}
# The gen of the listing the agent would send again, which LSSAME then names.
function Named {
    Quiet
    $listing = List '0'
    Assert ($listing.Gen) 'a host without the gen gets the listing with it'
    $same = List $listing.Gen
    Assert ($same.Same -ceq $listing.Gen -and -not $same.Enumerated) "LSSAME for an unchanged folder: $script:sent"
    return $listing.Gen
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
    $b64 = { param($text) [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($text)) }
    $script:BvLsrChanges = [long]-1
    $racing = List
    Assert (-not $racing.Entries.ContainsKey('late.txt')) 'the racing write is not in the listing it raced'
    Settle $from
    $after = List
    Assert ($after.Enumerated -and $after.Entries['late.txt'] -eq '2') 'a write during a listing is listed next time'
    $cases++

    # LSSAME (READY lssame): the unchanged folder is not sent; LSR without a gen is answered as
    # before; any change gets the listing with a new gen.
    $gen = Named
    $plain = List
    Assert (-not $plain.Enumerated -and $plain.Entries['late.txt'] -eq '2') 'without a gen: the cached listing as before'
    Assert ((List $gen).Same -ceq $gen) 'LSSAME again'
    $cases++
    Change { [IO.File]::WriteAllBytes((Join-Path $root 'same.bin'), [byte[]](1)) }
    $changed = List $gen
    Assert ($changed.Enumerated -and $changed.Gen -and $changed.Gen -cne $gen -and $changed.Entries['same.bin'] -eq '1') 'a change: the listing with a new gen'
    $cases++
    $gen = Named
    $script:BvLsrMaxAgeTicks = [long]1
    Start-Sleep -Milliseconds 5
    $aged = List $gen
    $script:BvLsrMaxAgeTicks = $maxAge
    Assert ($aged.Enumerated -and $aged.Gen -and $aged.Gen -cne $gen) 'past the age limit: the listing with a new gen'
    $cases++
    # A lost event (buffer overflow) or a dead watcher: its Error handler marks it broken.
    $gen = Named
    [BridgeVM.BvShareWatch].GetField('broken', [Reflection.BindingFlags]'NonPublic, Static').SetValue($null, $true)
    $lost = List $gen
    Assert ($lost.Enumerated -and $lost.Gen -and $lost.Gen -cne $gen) 'after a lost watcher event: the listing with a new gen'
    $cases++
    $gen = Named
    $arg = (& $b64 (Join-Path $root 'same.bin')) + ' ' + [Convert]::ToBase64String([byte[]](1))
    & $verbs['PUT']
    Assert ($script:sent -clike 'PUTOK *') "PUT reply: $script:sent"
    $written = List $gen
    Assert ($written.Enumerated -and $written.Gen -and $written.Gen -cne $gen) 'after its own write: the listing with a new gen'
    [IO.File]::Delete((Join-Path $root 'same.bin'))
    $cases++

    # The agent's own writes (the host's PUT and DEL) are listed by the very next LSR, before
    # the watcher's events for them arrive: the host takes that listing as its write landing.
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

    # A written file keeps the Mac file's time: 2026-11-08T00:05:12.123Z, as the host sends it.
    $macTime = [DateTime]::new(2026, 11, 8, 0, 5, 12, 123, [DateTimeKind]::Utc)
    $timed = Join-Path $root 'timed.bin'
    $arg = (& $b64 $timed) + ' ' + [Convert]::ToBase64String([byte[]](1, 2)) + ' 1794096312123'
    & $verbs['PUT']
    Assert ($script:sent -clike 'PUTOK *') "timed PUT reply: $script:sent"
    Assert ([IO.File]::GetLastWriteTimeUtc($timed) -eq $macTime) "PUT kept $([IO.File]::GetLastWriteTimeUtc($timed).ToString('o'))"
    $cases++
    $arg = (& $b64 $timed) + ' ' + [Convert]::ToBase64String([byte[]](3)) + ' not-a-time'
    & $verbs['PUT']
    Assert ($script:sent -clike 'PUTOK *' -and [IO.File]::ReadAllBytes($timed).Length -eq 1) "a bad time still writes: $script:sent"
    $left = [IO.File]::GetLastWriteTimeUtc($timed)
    Assert ([Math]::Abs(([DateTime]::UtcNow - $left).TotalMinutes) -lt 5) "a bad time leaves the write time, not $($left.ToString('o'))"
    $cases++
    $arg = (& $b64 $timed) + ' 3 1 1794096312123'
    & $verbs['PUTBEG']
    Assert ($script:sent -ceq 'OK PUTBEG') "PUTBEG reply: $script:sent"
    $arg = '0 ' + [Convert]::ToBase64String([byte[]](4, 5, 6))
    & $verbs['PUTCHUNK']
    $arg = '1'
    & $verbs['PUTEND']
    Assert ($script:sent -clike 'PUTOK *' -and [IO.File]::ReadAllBytes($timed).Length -eq 3) "PUTEND reply: $script:sent"
    Assert ([IO.File]::GetLastWriteTimeUtc($timed) -eq $macTime) "PUTEND kept $([IO.File]::GetLastWriteTimeUtc($timed).ToString('o'))"
    $cases++

    # Past 260 characters: Win32 refuses the path unless it starts with \\?\. One such file used to
    # fail every listing of the folder, and its PUT failed.
    if ([IO.Path]::DirectorySeparatorChar -eq '\') {
        $deep = (@(('d' * 60)) * 5) -join '\'
        $long = Join-Path $root ($deep + '\long.bin')
        Assert ($long.Length -gt 300) "long path is $($long.Length) characters"
        $arg = (& $b64 $long) + ' ' + [Convert]::ToBase64String([byte[]](5, 6)) + ' 1794096312123'
        & $verbs['PUT']
        Assert ($script:sent -clike 'PUTOK *') "long PUT reply: $script:sent"
        $listing = List
        Assert ($listing.Entries[($deep.Replace('\', '/') + '/long.bin')] -eq '2') "long path listed: $(@($listing.Entries.Keys) -join ',')"
        $arg = (& $b64 $long) + ' 3 1 1794096312123'
        & $verbs['PUTBEG']
        Assert ($script:sent -ceq 'OK PUTBEG') "long PUTBEG reply: $script:sent"
        $arg = '0 ' + [Convert]::ToBase64String([byte[]](7, 8, 9))
        & $verbs['PUTCHUNK']
        $arg = '1'
        & $verbs['PUTEND']
        Assert ($script:sent -clike 'PUTOK *') "long PUTEND reply: $script:sent"
        Assert ((List).Entries[($deep.Replace('\', '/') + '/long.bin')] -eq '3') 'long PUTBEG file listed'
        Assert ([IO.File]::GetLastWriteTimeUtc('\\?\' + $long) -eq $macTime) 'long PUTEND kept the time'
        $arg = & $b64 $long
        & $verbs['DEL']
        Assert ($script:sent -ceq 'OK DEL') "long DEL reply: $script:sent"
        Assert (-not (List).Entries.ContainsKey(($deep.Replace('\', '/') + '/long.bin'))) 'long path deleted'
        # Win32's name rules still apply: a trailing dot is trimmed, as for a short path.
        $dotted = Join-Path $root ($deep + '\dotted.')
        $arg = (& $b64 $dotted) + ' ' + [Convert]::ToBase64String([byte[]](1))
        & $verbs['PUT']
        Assert ($script:sent -clike 'PUTOK *') "long dotted PUT reply: $script:sent"
        Assert ((List).Entries[($deep.Replace('\', '/') + '/dotted')] -eq '1') 'a long dotted name is stored trimmed'
        $arg = & $b64 $dotted
        & $verbs['DEL']
        Assert ($script:sent -ceq 'OK DEL') "long dotted DEL reply: $script:sent"
        Assert (-not (List).Entries.ContainsKey(($deep.Replace('\', '/') + '/dotted'))) 'the trimmed file is deleted by its dotted name'
        $cases++
    }

    # Never older than the age limit, whatever the watcher says.
    $script:BvLsrMaxAgeTicks = [long]1
    Start-Sleep -Milliseconds 5
    Assert (List).Enumerated 'a listing past its age is enumerated again'
    $script:BvLsrMaxAgeTicks = $maxAge
    $cases++

    # The compiled listing gives the Get-ChildItem walk's rows. Under Windows PowerShell 5.1, the
    # agent's runtime, also its order: in each folder the folders, then the files, each in NTFS
    # order, then each folder below in that order (Windows CI, 2026-11-11). PowerShell 7 sorts
    # each folder by name instead, so there the rows are compared sorted. Names that sort apart
    # by culture and by code point, hidden and system files, an empty folder, a large PUT's
    # temporary (never listed) and look-alikes (listed), a long path on Windows, and a junction
    # (Windows) or a folder link (elsewhere), which is listed but not entered. A folder neither
    # can list fails both.
    $walkRoot = Join-Path ([IO.Path]::GetTempPath()) ('bv-walk-' + [Guid]::NewGuid().ToString('N'))
    $windows = [IO.Path]::DirectorySeparatorChar -eq '\'
    try {
        $names = @('b', 'A', '_u', 'a-b', 'Z', '[x]', '10', '9', ([string][char]0xD55C + [char]0xAE00), ([string][char]0xC4 + 'x'))
        foreach ($top in @('', 'A', 'A/_u', 'Z/deep/er')) {
            foreach ($n in $names) {
                $dir = Join-Path (Join-Path $walkRoot $top) ($n + '.d')
                [void][IO.Directory]::CreateDirectory($dir)
                [IO.File]::WriteAllBytes((Join-Path (Join-Path $walkRoot $top) ($n + '.txt')), (New-Object byte[] ($n.Length)))
            }
        }
        [void][IO.Directory]::CreateDirectory((Join-Path $walkRoot 'empty'))
        [IO.File]::WriteAllBytes((Join-Path $walkRoot ('A/.bridgevm-part-' + [Guid]::NewGuid().ToString('N'))), [byte[]](1))
        foreach ($n in @('.bridgevm-part-notes', ('.bridgevm-part-' + [Guid]::NewGuid().ToString('N').ToUpperInvariant()), '.hidden')) {
            [IO.File]::WriteAllBytes((Join-Path $walkRoot ('A/' + $n)), [byte[]](2, 2))
        }
        if ($windows) {
            # Precomposed and decomposed e-acute side by side (NTFS keeps both; one culture key).
            [IO.File]::WriteAllBytes((Join-Path $walkRoot ([string][char]0xE9)), [byte[]](1))
            [IO.File]::WriteAllBytes((Join-Path $walkRoot ('e' + [char]0x301)), [byte[]](1, 1))
            $hidden = Join-Path $walkRoot 'A/hidden-system.txt'
            [IO.File]::WriteAllBytes($hidden, [byte[]](3))
            [IO.File]::SetAttributes($hidden, [IO.FileAttributes]'Hidden, System')
            [IO.File]::SetAttributes((Join-Path $walkRoot 'Z'), [IO.FileAttributes]'Directory, Hidden')
            $deepDir = Join-Path $walkRoot ((@(('w' * 60)) * 5) -join '\')
            [void][IO.Directory]::CreateDirectory('\\?\' + $deepDir)
            [IO.File]::WriteAllBytes('\\?\' + $deepDir + '\long.bin', [byte[]](4))
            [void](New-Item -ItemType Junction -Path (Join-Path $walkRoot 'link') -Value (Join-Path $walkRoot 'A/_u'))
        } else {
            [void](New-Item -ItemType SymbolicLink -Path (Join-Path $walkRoot 'link') -Value (Join-Path $walkRoot 'A/_u'))
        }
        $rootFull = [IO.Path]::GetFullPath($walkRoot) + [IO.Path]::DirectorySeparatorChar
        $listRoot = Get-IoPath $rootFull $true
        function Listing([bool]$compiled) {
            $script:BvShareWatchReady = $compiled
            try { & $listShare $listRoot } finally { $script:BvShareWatchReady = $true }
        }
        # A listing shows a folder's time from its parent's entry, which NTFS brings up to date
        # only after the folder changed and was opened: the first listing after the tree was made
        # (whichever way) moved the times of folders the test had just filled (CI: A, the long
        # path's folders, Z\deep\er, about 2 ms). The agent lists a folder its watcher saw settle;
        # here the tree is listed until two walks agree, then compared.
        $walked = Listing $false
        for ($i = 0; $i -lt 10; $i++) {
            $again = Listing $false
            if ($again -ceq $walked) { break }
            $walked = $again
            Start-Sleep -Milliseconds 200
        }
        $compiled = Listing $true
        Assert ((Listing $false) -ceq $walked) 'the tree held still while it was compared'
        if ($PSVersionTable.PSEdition -ne 'Desktop') {
            $sort = { param($text) $rows = [string[]]($text -split "`r?`n"); [Array]::Sort($rows, [StringComparer]::Ordinal); $rows -join "`n" }
            $compiled = & $sort $compiled
            $walked = & $sort $walked
        }
        $a = $compiled -split "`r?`n"; $b = $walked -split "`r?`n"
        $at = 0; while ($at -lt [Math]::Min($a.Count, $b.Count) -and $a[$at] -ceq $b[$at]) { $at++ }
        if (-not ($compiled -ceq $walked)) {
            $onlyA = @($a | Where-Object { $b -cnotcontains $_ } | Select-Object -First 25)
            $onlyB = @($b | Where-Object { $a -cnotcontains $_ } | Select-Object -First 25)
            Write-Host ('only compiled (' + $onlyA.Count + '): ' + ($onlyA -join ' ;; '))
            Write-Host ('only walked (' + $onlyB.Count + '): ' + ($onlyB -join ' ;; '))
            Write-Host ('compiled order: ' + (($a | Select-Object -First 30) -join ' ;; '))
            Write-Host ('walked order: ' + (($b | Select-Object -First 30) -join ' ;; '))
        }
        Assert ($compiled -ceq $walked) "compiled and walked listings differ at row $at of $($a.Count)/$($b.Count): '$(if ($at -lt $a.Count) { $a[$at] })' vs '$(if ($at -lt $b.Count) { $b[$at] })'"
        Assert ($a.Count -gt 60 -and $compiled -cmatch '\.bridgevm-part-notes\|2\|0\|' -and $compiled -cnotmatch 'bridgevm-part-[0-9a-f]{32}\|') "the compiled listing: $($a.Count) rows"
        Assert ($compiled -cmatch '(?m)^link\|1\|1\|' -and $compiled -cnotmatch '(?m)^link[\\/]') 'the link is listed, not entered'
        $cases++

        $locked = Join-Path $walkRoot 'A/locked'
        [void][IO.Directory]::CreateDirectory($locked)
        $deny = if ($windows) { { icacls $locked /deny '*S-1-1-0:(RD)' | Out-Null } } else { { chmod 000 $locked } }
        $allow = if ($windows) { { icacls $locked /remove:d '*S-1-1-0' | Out-Null } } else { { chmod 755 $locked } }
        & $deny
        try {
            $failed = foreach ($ready in @($true, $false)) {
                $script:BvShareWatchReady = $ready
                try { [void](& $listShare $listRoot); 'listed' } catch { 'failed' }
            }
        } finally { & $allow; $script:BvShareWatchReady = $true }
        # (root lists any folder off Windows)
        if ($windows -or (& id -u) -ne '0') {
            Assert (($failed -join ',') -ceq 'failed,failed') "a folder that cannot be listed fails both: $($failed -join ',')"
        }
        $cases++
    } finally {
        $script:BvShareWatchReady = $true
        # The link itself, before the tree, which must not be emptied through it.
        $link = Join-Path $walkRoot 'link'
        if ($windows -and [IO.Directory]::Exists($link)) { [IO.Directory]::Delete($link) } elseif (-not $windows) { [IO.File]::Delete($link) }
        Remove-Item -LiteralPath (Get-IoPath $walkRoot $true) -Recurse -Force -ErrorAction SilentlyContinue
    }

    # Without the watcher every listing enumerates, as before, and a gen never comes back as LSSAME.
    $script:BvShareWatchReady = $false
    Assert ((List).Enumerated -and (List).Enumerated) 'without the watcher every LSR enumerates'
    $unwatched = List '0'
    $again = List $unwatched.Gen
    Assert ($again.Enumerated -and $again.Gen -and $again.Gen -cne $unwatched.Gen) 'without the watcher a gen gets the listing again'
    $cases++
} finally {
    Remove-Item -LiteralPath (Get-IoPath $root $true) -Recurse -Force -ErrorAction SilentlyContinue
}
Write-Output "agent share listing contract: PASS ($cases cases, $($PSVersionTable.PSEdition))"
