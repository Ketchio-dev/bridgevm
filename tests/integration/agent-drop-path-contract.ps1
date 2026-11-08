# The guest agent's Finder-drop verbs on paths past Win32's 260 characters: DROPDIR and DROPBEG
# create a dropped folder's deepest items, a repeated name becomes "name (2)", and DROPABORT deletes
# its half-written file. Before the fix one such path failed the whole drop. Runs the agent's own
# verb cases against a temporary folder that stands in for the Desktop.
$ErrorActionPreference = 'Stop'
$repo = Split-Path (Split-Path $PSScriptRoot -Parent) -Parent
$source = [IO.File]::ReadAllText((Join-Path $repo 'scripts/win-assets/bvagent.ps1'))
foreach ($name in @('Get-IoPath', 'Test-DropName', 'Resolve-DropTarget', 'Close-DropStream')) {
    $function = [regex]::Match($source, "(?s)\nfunction $name\b.*?\r?\n\}\r?\n")
    if (-not $function.Success) { throw "$name not found" }
    . ([scriptblock]::Create($function.Value))
}
$verbs = @{}
foreach ($verb in @('DROPDIR', 'DROPBEG', 'DROPEND', 'DROPABORT')) {
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
$script:BvDropReader = $false
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
    Assert (Exists (Join-Path $deep 'leaf (2).txt')) 'leaf (2).txt was not opened'
    Assert ((Send 'DROPABORT' '') -ceq 'OK DROPABORT') "DROPABORT: $script:sent"
    Assert (-not (Exists (Join-Path $deep 'leaf (2).txt'))) 'DROPABORT left leaf (2).txt behind'
    Assert (Exists (Join-Path $deep 'leaf.txt')) 'DROPABORT deleted leaf.txt'
    $cases++

    # The deepest folder dropped again becomes "<name> (2)".
    $reply = Send 'DROPDIR' (B64 $rel)
    Assert ($reply -clike 'OK DROPDIR *' -and (Name $reply) -ceq ($part + ' (2)')) "DROPDIR again: $reply"
    $cases++
} finally {
    Close-DropStream $false
    if (Test-Path -LiteralPath $root) { [IO.Directory]::Delete('\\?\' + $root, $true) }
}
Write-Output "agent drop path contract: PASS ($cases cases, $($PSVersionTable.PSEdition))"
