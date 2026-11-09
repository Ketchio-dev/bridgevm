# The guest agent's Finder-drop verbs on paths past Win32's 260 characters: DROPDIR and DROPBEG
# create a dropped folder's deepest items, a repeated name becomes "name (2)", and DROPABORT deletes
# its half-written file. Before the fix one such path failed the whole drop. A file is written to a
# staging folder and gets its Desktop name only at DROPEND, so a drop the agent never finished
# leaves nothing under a real name, and the next drop job clears what it left. Runs the agent's own
# verb cases against temporary folders that stand in for the Desktop and the staging folder.
$ErrorActionPreference = 'Stop'
$repo = Split-Path (Split-Path $PSScriptRoot -Parent) -Parent
$source = [IO.File]::ReadAllText((Join-Path $repo 'scripts/win-assets/bvagent.ps1'))
foreach ($name in @('Get-IoPath', 'Test-DropName', 'Get-FreeDropPath', 'Resolve-DropTarget', 'Close-DropStream', 'Clear-DropStaging', 'Get-DropVolume', 'Select-DropStaging')) {
    $function = [regex]::Match($source, "(?s)\nfunction $name\b.*?\r?\n\}\r?\n")
    if (-not $function.Success) { throw "$name not found" }
    . ([scriptblock]::Create($function.Value))
}
$verbs = @{}
foreach ($verb in @('DROPCAPS', 'DROPDIR', 'DROPBEG', 'DROPEND', 'DROPABORT')) {
    # A one-line case first (DROPABORT); the multi-line form would run on into the next case.
    $one = [regex]::Matches($source, "(?m)^                '$verb' \{(?<body>.*)\}\r?$")
    if ($one.Count -ne 1) {
        $one = [regex]::Matches($source, "(?s)\n                '$verb' \{(?<body>.*?)\r?\n                \}\r?\n")
    }
    if ($one.Count -ne 1) { throw "expected exactly one $verb implementation" }
    $verbs[$verb] = [scriptblock]::Create($one[0].Groups['body'].Value)
}
function Log([string]$m) { }
function Write-Line($handle, [string]$line, [string]$label) { $script:sent = $line }
function Assert($condition, [string]$what) { if (-not $condition) { throw "FAIL: $what" } }
function B64([string]$text) { [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($text)) }
function Name([string]$reply) { [Text.Encoding]::UTF8.GetString([Convert]::FromBase64String($reply.Split(' ')[2])) }
function Send([string]$verb, [string]$arg) {
    $script:sent = $null
    $h = [IntPtr]::Zero
    & $verbs[$verb]
    return $script:sent
}
function Exists([string]$path) { [IO.File]::Exists('\\?\' + $path) }

$root = Join-Path ([IO.Path]::GetTempPath()) ('bv-drop-' + [Guid]::NewGuid().ToString('N'))
[void][IO.Directory]::CreateDirectory($root)
function Get-DropRoot { return $root }
$staging = Join-Path ([IO.Path]::GetTempPath()) ('bv-drop-staging-' + [Guid]::NewGuid().ToString('N'))
[void][IO.Directory]::CreateDirectory($staging)
function Get-DropStaging { return $staging }
function Get-DropStagingDirs { return @($staging) }
function Test-DropReader { return $false }
function Staged { @([IO.Directory]::GetFiles($staging)).Count }
$script:BvDropReader = $false
$script:BvDropTemp = ''
$cases = 0
try {
    # Five folders of 50 characters: the deepest is past 260 characters below any Desktop.
    $rel = 'deep'
    Assert ((Send 'DROPDIR' (B64 $rel)) -ceq ('OK DROPDIR ' + (B64 'deep'))) "DROPDIR deep: $script:sent"
    for ($i = 1; $i -le 5; $i++) {
        $part = ('level{0}-' -f $i) + ('a' * 43)
        $rel = $rel + '\' + $part
        $reply = Send 'DROPDIR' (B64 $rel)
        Assert ($reply -clike 'OK DROPDIR *' -and (Name $reply) -ceq $part) "DROPDIR level ${i}: $reply"
    }
    $deep = Join-Path $root $rel
    Assert ($deep.Length -gt 260) "the deepest folder is only $($deep.Length) characters"
    Assert ([IO.Directory]::Exists('\\?\' + $deep)) 'the deepest folder was not created'
    $cases++

    # An empty file there, then the same name again: "leaf (2).txt", which DROPABORT deletes.
    $reply = Send 'DROPBEG' ((B64 ($rel + '\leaf.txt')) + ' 0')
    Assert ($reply -ceq ('OK DROPBEG ' + (B64 'leaf.txt'))) "DROPBEG leaf: $reply"
    Assert ((Send 'DROPEND' '0') -ceq 'OK DROPEND 0') "DROPEND leaf: $script:sent"
    Assert (Exists (Join-Path $deep 'leaf.txt')) 'leaf.txt was not written'
    $cases++
    $reply = Send 'DROPBEG' ((B64 ($rel + '\leaf.txt')) + ' 5')
    Assert ($reply -ceq ('OK DROPBEG ' + (B64 'leaf (2).txt'))) "DROPBEG again: $reply"
    Assert (-not (Exists (Join-Path $deep 'leaf (2).txt'))) 'an unfinished drop is already at its Desktop name'
    Assert ((Staged) -eq 1) 'the unfinished drop is not staged'
    Assert ((Send 'DROPABORT' '') -ceq 'OK DROPABORT') "DROPABORT: $script:sent"
    Assert (-not (Exists (Join-Path $deep 'leaf (2).txt'))) 'DROPABORT left leaf (2).txt behind'
    Assert ((Staged) -eq 0) 'DROPABORT left its staged copy'
    Assert (Exists (Join-Path $deep 'leaf.txt')) 'DROPABORT deleted leaf.txt'
    $cases++

    # Written whole: it gets its name at DROPEND, with its bytes.
    Assert ((Send 'DROPBEG' ((B64 'full.txt') + ' 3')) -ceq ('OK DROPBEG ' + (B64 'full.txt'))) "DROPBEG full: $script:sent"
    $script:BvDropStream.Write([byte[]](97, 98, 99), 0, 3); $script:BvDropWritten = 3; $script:BvDropSeq = 1
    Assert ((Send 'DROPEND' '1') -ceq 'OK DROPEND 3') "DROPEND full: $script:sent"
    Assert ([IO.File]::ReadAllText((Join-Path $root 'full.txt')) -ceq 'abc') 'full.txt is not its three bytes'
    Assert ((Staged) -eq 0) 'DROPEND left its staged copy'
    $cases++

    # A file made at the name since DROPBEG keeps it; the drop takes the next free name.
    Assert ((Send 'DROPBEG' ((B64 'race.txt') + ' 0')) -ceq ('OK DROPBEG ' + (B64 'race.txt'))) "DROPBEG race: $script:sent"
    [IO.File]::WriteAllText((Join-Path $root 'race.txt'), 'mine')
    Assert ((Send 'DROPEND' '0') -ceq 'OK DROPEND 0') "DROPEND race: $script:sent"
    Assert ([IO.File]::ReadAllText((Join-Path $root 'race.txt')) -ceq 'mine') 'the drop replaced a file made meanwhile'
    Assert (Exists (Join-Path $root 'race (2).txt')) 'the drop did not take the next free name'
    $cases++

    # The agent stops mid-drop (a guest restart): nothing at the name, and the next job's DROPCAPS
    # clears the staged copy the new agent knows nothing about.
    Assert ((Send 'DROPBEG' ((B64 'cut.bin') + ' 5')) -clike 'OK DROPBEG *') "DROPBEG cut: $script:sent"
    $script:BvDropStream.Write([byte[]](1, 2), 0, 2)
    $script:BvDropStream.Close(); $script:BvDropStream = $null; $script:BvDropTemp = ''; $script:BvDropPath = ''
    Assert (-not (Exists (Join-Path $root 'cut.bin'))) 'a cut-short drop is at its Desktop name'
    [IO.File]::WriteAllText((Join-Path $staging 'notes.txt'), 'not ours')
    Assert ((Send 'DROPCAPS' '') -ceq 'BVDROP_CAPS 1') "DROPCAPS: $script:sent"
    Assert ((Staged) -eq 1 -and [IO.File]::Exists((Join-Path $staging 'notes.txt'))) 'DROPCAPS did not clear exactly the staged copies'
    $cases++

    # The staging folder is on the target's volume, so DROPEND's move is a rename: the user's temp
    # folder when it shares it, else a folder on the (relocated or redirected) Desktop itself.
    $temp = 'C:\Users\u\AppData\Local\Temp\'
    Assert ((Select-DropStaging $temp 'C:\Users\u\Desktop' 'c:\Users\u\Desktop\a.txt') -ceq [IO.Path]::Combine($temp, 'BridgeVM-drop')) 'same volume: not the temp folder'
    Assert ((Select-DropStaging $temp 'D:\Desk' 'D:\Desk\sub\a.txt') -ceq [IO.Path]::Combine('D:\Desk', '.bridgevm-drop')) 'relocated Desktop: not on its volume'
    Assert ((Select-DropStaging $temp '\\srv\home\u\Desktop' '\\srv\home\u\Desktop\a.txt') -ceq [IO.Path]::Combine('\\srv\home\u\Desktop', '.bridgevm-drop')) 'redirected Desktop: not on its share'
    Assert ((Get-DropVolume '\\srv\home\u') -ceq (Get-DropVolume '\\SRV\Home\v')) 'one share, two volumes'
    $cases++

    # The deepest folder dropped again becomes "<name> (2)".
    $reply = Send 'DROPDIR' (B64 $rel)
    Assert ($reply -clike 'OK DROPDIR *' -and (Name $reply) -ceq ($part + ' (2)')) "DROPDIR again: $reply"
    $cases++
} finally {
    Close-DropStream $false
    if (Test-Path -LiteralPath $root) { [IO.Directory]::Delete('\\?\' + $root, $true) }
    if (Test-Path -LiteralPath $staging) { [IO.Directory]::Delete($staging, $true) }
}
Write-Output "agent drop path contract: PASS ($cases cases, $($PSVersionTable.PSEdition))"
