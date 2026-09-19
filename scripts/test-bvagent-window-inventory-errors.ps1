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
$assembly = [Reflection.Emit.AssemblyBuilder]::DefineDynamicAssembly(
    [Reflection.AssemblyName]::new('BridgeVM.InventoryFailureFixture'), [Reflection.Emit.AssemblyBuilderAccess]::Run)
$type = $assembly.DefineDynamicModule('main').DefineType('BridgeVM.NativeWindowInventory',
    [Reflection.TypeAttributes]'Public, Abstract, Sealed')
$method = $type.DefineMethod('Enumerate', [Reflection.MethodAttributes]'Public, Static', [object[]], [Type[]]@())
$il = $method.GetILGenerator()
$il.Emit([Reflection.Emit.OpCodes]::Ldstr, 'fixture provider failure')
$il.Emit([Reflection.Emit.OpCodes]::Newobj, [InvalidOperationException].GetConstructor([Type[]]@([string])))
$il.Emit([Reflection.Emit.OpCodes]::Throw)
$null = $type.CreateType()
Get-BvWindowInventoryLines
'@
    missing_provider = 'Get-BvWindowInventoryLines'
}
try {
    foreach ($name in $cases.Keys) {
        $script = Join-Path $root ($name + '.ps1')
        [IO.File]::WriteAllText($script, $prefix + $cases[$name] + "`nWrite-Output 'UNREACHED'`n", [Text.Encoding]::UTF8)
        $result = Invoke-BvInventoryErrorChild -Run $run -Name $name -Script $script
        if ($name -eq 'native_failure' -and $result.Error -notmatch 'fixture provider failure') { throw 'Native failure fixture did not reach the emitted provider' }
    }
    $failed = $false
} finally { Complete-BvInventoryErrorRun $run $root $failed }
Write-Output 'PASS: unguarded Continue cannot leak inventory rows or hide provider failures'
