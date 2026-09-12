$ErrorActionPreference = 'Stop'
$root = Join-Path ([IO.Path]::GetTempPath()) ('BridgeVM inventory errors ' + [guid]::NewGuid())
New-Item -ItemType Directory -Path $root | Out-Null
$engine = (Get-Process -Id $PID).Path
$module = Join-Path $root 'bvagent-window-inventory.ps1'
Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'win-assets/bvagent-window-inventory.ps1') -Destination $module
$prefix = '$ErrorActionPreference = ''Continue''' + "`n. '" + $module.Replace("'", "''") + "'`n"
$cases = @{
    invalid_utf8 = @'
$row = [pscustomobject]@{Handle='42';ProcessId=7;X=0;Y=0;W=10;H=10;Title=([string][char]0xd800)}
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
        $start = [Diagnostics.ProcessStartInfo]::new()
        $start.FileName = $engine
        $start.Arguments = '-NoLogo -NoProfile -File "' + $script + '"'
        $start.UseShellExecute = $false
        $start.RedirectStandardOutput = $true
        $start.RedirectStandardError = $true
        $child = [Diagnostics.Process]::Start($start)
        try {
            if (-not $child.WaitForExit(10000)) { $child.Kill(); throw ($name + ' child deadline') }
            $stdout = $child.StandardOutput.ReadToEnd()
            $stderr = $child.StandardError.ReadToEnd()
            if ($child.ExitCode -eq 0 -or $stdout.Length -ne 0 -or $stderr.Length -eq 0) {
                throw ($name + ' did not fail closed under unguarded Continue')
            }
        } finally { $child.Dispose() }
    }
} finally { Remove-Item -LiteralPath $root -Recurse -Force }
Write-Output 'PASS: unguarded Continue cannot leak inventory rows or hide provider failures'
