$ErrorActionPreference = 'Stop'
$tokens = $null; $errors = $null
$source = Join-Path $PSScriptRoot 'win-assets/bvagent-firstboot.ps1'
$ast = [Management.Automation.Language.Parser]::ParseFile($source, [ref]$tokens, [ref]$errors)
if ($errors.Count -ne 0) { throw 'firstboot parse failure' }
$function = $ast.Find({ param($node)
    $node -is [Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -eq 'Confirm-GuestAgentAssets'
}, $true)
if ($null -eq $function) { throw 'firstboot asset verifier missing' }
. ([scriptblock]::Create($function.Extent.Text))
$root = Join-Path ([IO.Path]::GetTempPath()) ([Guid]::NewGuid().ToString('N'))
try {
    [void](New-Item -ItemType Directory -Path (Join-Path $root 'agent') -Force)
    $records = @()
    foreach ($name in @('bvagent.ps1', 'bvagent-input.ps1', 'bvagent-unicode-input.cs', 'bvagent-key-input.cs', 'bvagent-pointer-input.cs', 'bvagent-window-inventory.ps1', 'bv-window-inventory.cs', 'bvagent-task.ps1')) {
        $path = Join-Path $root ('agent/' + $name)
        [IO.File]::WriteAllText($path, 'synthetic asset ' + $name)
        $hash = (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant()
        $records += ,@('guest_tool', ('agent/' + $name), $hash)
    }
    $verified = Confirm-GuestAgentAssets $records $root
    if ($verified -cne $records[0][2]) { throw 'valid asset set failed verification' }
    for ($index = 0; $index -lt $records.Count; $index++) {
        $path = Join-Path $root $records[$index][1]
        $original = [IO.File]::ReadAllText($path)
        [IO.File]::WriteAllText($path, 'mutated')
        $blocked = $false
        try { Confirm-GuestAgentAssets $records $root | Out-Null } catch { $blocked = $true }
        if (-not $blocked) { throw 'mutated guest code was accepted' }
        [IO.File]::WriteAllText($path, $original)
        Remove-Item -LiteralPath $path
        $blocked = $false
        try { Confirm-GuestAgentAssets $records $root | Out-Null } catch { $blocked = $true }
        if (-not $blocked) { throw 'missing guest code was accepted' }
        [IO.File]::WriteAllText($path, $original)
    }
    foreach ($invalid in @(@($records[0..($records.Count - 2)]), @($records + ,$records[-1]))) {
        $blocked = $false
        try { Confirm-GuestAgentAssets $invalid $root | Out-Null } catch { $blocked = $true }
        if (-not $blocked) { throw 'incomplete/duplicate guest identity was accepted' }
    }
    Write-Output 'Firstboot input asset identities: PASS (no scheduled task created)'
} finally {
    if (Test-Path -LiteralPath $root) { Remove-Item -LiteralPath $root -Recurse -Force }
}
