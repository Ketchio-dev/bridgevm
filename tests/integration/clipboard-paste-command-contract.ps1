param([Parameter(Mandatory = $true)][string]$FixturePath)
$ErrorActionPreference = 'Stop'
$fixtures = ConvertFrom-Json -InputObject (Get-Content -Raw -Encoding UTF8 -LiteralPath $FixturePath)
if ($fixtures.Count -ne 4) { throw 'Expected four generated Swift fixtures' }
function Set-Clipboard {
    [CmdletBinding()]
    param([string]$Value)
    $script:calls += 1
    if ($script:failSet) { Write-Error 'fixture clipboard busy'; return }
    $script:written = $Value
}
$prefix = 'powershell -NoProfile -STA -Command "'
$markers = @{}
$passed = 0
foreach ($fixture in $fixtures) {
    $command = [string]$fixture.command
    if (-not $command.StartsWith($prefix) -or -not $command.EndsWith('"')) {
        throw 'Unsupported generated process wrapper'
    }
    if ($fixture.marker -notmatch '^BVPASTE_READY [0-9A-F-]{36}$' -or $markers.ContainsKey($fixture.marker)) {
        throw 'Missing or duplicate request identity'
    }
    $markers[$fixture.marker] = $true
    $body = [scriptblock]::Create($command.Substring($prefix.Length, $command.Length - $prefix.Length - 1))
    foreach ($failSet in @($false, $true)) {
        $script:failSet = $failSet
        $script:written = 'unchanged'
        $script:calls = 0
        $failed = $false
        $output = @()
        try { $output = @(& $body) } catch { $failed = $true }
        if ($script:calls -ne 1) { throw 'Expected exactly one clipboard operation' }
        if ($failSet) {
            if (-not $failed -or $output.Count -ne 0 -or $script:written -cne 'unchanged') {
                throw 'Failed clipboard write emitted success or changed the value'
            }
        } elseif ($failed -or $script:written -cne $fixture.text -or $output.Count -ne 1 -or
                  $output[0] -cne $fixture.marker) {
            throw 'Generated command did not preserve text and acknowledge exactly'
        }
        $passed += 1
    }
}
Write-Output "generated clipboard paste command: PASS ($passed cases)"
