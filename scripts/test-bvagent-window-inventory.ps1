$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'win-assets/bvagent-window-inventory.ps1')
function New-Row([string]$handle = '42', [string]$title = 'one') {
    [pscustomobject]@{ Handle=$handle; ProcessId=7; X=-10; Y=20; Width=300; Height=180; Title=$title }
}
function Assert-True([bool]$value, [string]$message) { if (-not $value) { throw $message } }
function Expect-Rejected([object[]]$rows) {
    $seen = [Collections.Generic.List[string]]::new()
    $failed = $false
    try { ConvertTo-BvWindowInventoryLines -Rows $rows | ForEach-Object { $seen.Add($_) } }
    catch { $failed = $true }
    Assert-True $failed 'Invalid inventory was accepted'
    Assert-True ($seen.Count -eq 0) 'A partial inventory escaped before rejection'
}
$first = New-Row
$second = New-Row '43' ([string][char]0xd55c + [char]0xae00 + [char]0xd83d + [char]0xde42)
$lines = @(ConvertTo-BvWindowInventoryLines -Rows @($first, $second))
$encoded = [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($second.Title))
Assert-True ($lines.Count -eq 2) 'Same-process windows were collapsed'
Assert-True ($lines[0] -ceq 'WIN 42 7 -10 20 300 180 b25l') 'Field order changed'
Assert-True ($lines[1] -ceq ('WIN 43 7 -10 20 300 180 ' + $encoded)) 'Unicode bytes changed'
Assert-True (@(ConvertTo-BvWindowInventoryLines -Rows @()).Count -eq 0) 'Empty inventory failed'
Expect-Rejected $null
Expect-Rejected @($first, $first)
foreach ($handle in @('0', '-1', '01', '18446744073709551616', '42 43')) {
    Expect-Rejected @($first, (New-Row $handle))
}
foreach ($field in @('ProcessId', 'X', 'Y', 'Width', 'Height')) {
    foreach ($value in @('not-an-integer', '4294967296')) {
        $bad = New-Row '44'; $bad.$field = $value
        Expect-Rejected @($first, $bad)
    }
}
foreach ($field in @('ProcessId', 'Width', 'Height')) {
    $bad = New-Row '44'; $bad.$field = 0; Expect-Rejected @($first, $bad)
}
foreach ($title in @('', ([string][char]0xd800), ('x' * 3145728))) {
    Expect-Rejected @($first, (New-Row '44' $title))
}
$many = @(1..4096 | ForEach-Object { New-Row ([string]$_) })
Assert-True (@(ConvertTo-BvWindowInventoryLines -Rows $many).Count -eq 4096) 'Row ceiling changed'
Expect-Rejected ($many + @(New-Row '4097'))
$savedCulture = [Threading.Thread]::CurrentThread.CurrentCulture
try {
    [Threading.Thread]::CurrentThread.CurrentCulture = [Globalization.CultureInfo]::GetCultureInfo('fr-FR')
    Assert-True (@(ConvertTo-BvWindowInventoryLines -Rows @($first))[0] -ceq $lines[0]) 'Culture changed wire'
} finally { [Threading.Thread]::CurrentThread.CurrentCulture = $savedCulture }
Write-Output 'PASS: complete bounded Coherence wire formatting (not resident-agent or live proof)'
