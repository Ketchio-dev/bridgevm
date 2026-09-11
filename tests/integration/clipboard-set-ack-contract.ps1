$ErrorActionPreference = 'Stop'
$root = Split-Path (Split-Path $PSScriptRoot -Parent) -Parent
$source = [IO.File]::ReadAllText((Join-Path $root 'scripts/win-assets/bvagent.ps1'))
$pattern = '(?s)                ''CLIPSET'' \{(?<body>.*?)\r?\n                \}\r?\n                ''LS'''
$matches = [regex]::Matches($source, $pattern)
if ($matches.Count -ne 1) { throw 'Expected exactly one CLIPSET implementation' }
$body = [scriptblock]::Create($matches[0].Groups['body'].Value)
Add-Type 'public static class ClipboardAckFixture { public static uint GetClipboardSequenceNumber() { return 8; } }'
$K = [ClipboardAckFixture]
$h = [IntPtr]::Zero
function Write-Line($handle, [string]$line, [string]$label) { $line }
function Set-Clipboard {
    [CmdletBinding()]
    param([string]$Value)
    $script:calls += 1
    if ($script:mode -eq 'nonterminating') { Write-Error 'clipboard busy'; return }
    if ($script:mode -eq 'terminating') { throw 'clipboard unavailable' }
    $script:written = $Value
}
$passed = 0
foreach ($case in @('success', 'nonterminating', 'terminating', 'invalid-base64')) {
    $script:mode = $case
    $script:calls = 0
    $script:written = 'unchanged'
    $script:BvClipCache = 'prior-cache'
    $script:BvClipSeq = 7
    $text = 'clipboard-' + [char]0xD55C + [char]0xAE00
    $arg = [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($text))
    if ($case -eq 'invalid-base64') { $arg = '%%%invalid%%%' }
    # Match the resident agent's preference; the command must opt into Stop.
    $ErrorActionPreference = 'Continue'
    $reply = @(& $body)
    $ErrorActionPreference = 'Stop'
    if ($reply.Count -ne 1) { throw "$case returned an ambiguous response" }
    if ($case -eq 'success') {
        if ($reply[0] -cne 'OK CLIPSET' -or $script:written -cne $text -or
            $script:BvClipCache -cne $text -or $script:BvClipSeq -ne 8 -or $script:calls -ne 1) {
            throw 'Successful clipboard set was not acknowledged and cached exactly'
        }
    } else {
        if ($reply[0] -notlike 'ERR CLIPSET *' -or $script:BvClipCache -cne 'prior-cache' -or
            $script:BvClipSeq -ne 7 -or $script:written -cne 'unchanged') {
            throw "$case falsely acknowledged or changed cached clipboard state"
        }
        $expectedCalls = if ($case -eq 'invalid-base64') { 0 } else { 1 }
        if ($script:calls -ne $expectedCalls) { throw "$case unexpected clipboard call count" }
    }
    $passed += 1
}
Write-Output "clipboard set acknowledgment: PASS ($passed cases)"
