$ErrorActionPreference = 'Stop'
$integration = Join-Path $PSScriptRoot '../tests/integration'
. (Join-Path $integration 'window-inventory-error-child.ps1')
& (Join-Path $integration 'window-inventory-owned-child-contract.ps1')
$root = Join-Path ([IO.Path]::GetTempPath()) ('BridgeVM inventory errors ' + [guid]::NewGuid())
New-Item -ItemType Directory -Path $root | Out-Null
$run = New-B6TipEvidenceRun; $failed = $true
$module = Join-Path $root 'bvagent-window-inventory.ps1'
Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'win-assets/bvagent-window-inventory.ps1') -Destination $module
$prefix = '$ErrorActionPreference = ''Continue''' + "`n. '" + $module.Replace("'", "''") + "'`n"
$cases = @{
    invalid_utf8 = @'
$row = [pscustomobject]@{Handle='42';ProcessId=7;X=0;Y=0;Width=10;Height=10;Title=([string][char]0xd800)}
ConvertTo-BvWindowInventoryLines -Rows @($row)
'@
    native_failure = @'
Add-Type -TypeDefinition 'namespace BridgeVM { public static class NativeWindowInventory { public static object[] Enumerate() { throw new System.InvalidOperationException("fixture provider failure"); } } }'
Get-BvWindowInventoryLines
'@
    missing_provider = 'Get-BvWindowInventoryLines'
}
try {
    foreach ($name in $cases.Keys) {
        $script = Join-Path $root ($name + '.ps1')
        [IO.File]::WriteAllText($script, $prefix + $cases[$name] + "`nWrite-Output 'UNREACHED'`n", [Text.Encoding]::UTF8)
        $null = Invoke-BvInventoryErrorChild -Run $run -Name $name -Script $script
    }
    $failed = $false
} finally { Complete-BvInventoryErrorRun $run $root $failed }
Write-Output 'PASS: unguarded Continue cannot leak inventory rows or hide provider failures'
