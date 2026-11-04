$ErrorActionPreference = 'Stop'
# Applies the committed provisioner's Lock-BridgeVMFolder to a folder under C:\ laid out like
# C:\BridgeVM after WinPE's xcopy (everything inheriting C:\'s defaults), plus a file with its own
# Everyone entry and one with a protected copy of its inherited entries, then reads the permissions
# back. A root that is a junction must be refused before anything changes; a junction below it must
# be refused with the root locked first, and the folder it points to (outside) left as it was.
$tokens = $null; $errors = $null
$source = Join-Path $PSScriptRoot '../../scripts/win-assets/bvagent-firstboot.ps1'
$ast = [Management.Automation.Language.Parser]::ParseFile((Resolve-Path -LiteralPath $source).Path, [ref]$tokens, [ref]$errors)
if ($errors.Count -ne 0) { throw 'firstboot parse failure' }
$function = $ast.Find({ param($node) $node -is [Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -eq 'Lock-BridgeVMFolder' }, $true)
if ($null -eq $function) { throw 'firstboot has no Lock-BridgeVMFolder' }
. ([scriptblock]::Create($function.Extent.Text))

$sid = [Security.Principal.SecurityIdentifier]
$roots = @(); $links = @()
function New-Root {
    $root = Join-Path $env:SystemDrive ('bvagent-acl-' + [guid]::NewGuid().ToString('N'))
    $script:roots += $root
    foreach ($dir in 'provisioning\agent', 'provisioning\drivers') { [void](New-Item -ItemType Directory -Path (Join-Path $root $dir)) }
    foreach ($file in 'guest-tools-firstboot.log', 'provisioning\payload-receipt.tsv', 'provisioning\drivers\vioser.inf', 'provisioning\agent\bvagent.ps1', 'provisioning\agent\explicit.cs', 'provisioning\agent\protected.ps1') {
        [IO.File]::WriteAllText((Join-Path $root $file), $file)
    }
    return $root
}
function Show-Icacls([string]$Root) {
    foreach ($path in @($Root) + @(Get-ChildItem -LiteralPath $Root -Recurse -Force | ForEach-Object { $_.FullName })) { icacls $path | Select-Object -First 6 }
}
try {
    $root = New-Root
    $acl = Get-Acl -LiteralPath (Join-Path $root 'provisioning\agent\explicit.cs')
    $acl.AddAccessRule((New-Object Security.AccessControl.FileSystemAccessRule((New-Object Security.Principal.SecurityIdentifier 'S-1-1-0'), 'Modify', 'Allow')))
    Set-Acl -LiteralPath (Join-Path $root 'provisioning\agent\explicit.cs') -AclObject $acl
    $acl = Get-Acl -LiteralPath (Join-Path $root 'provisioning\agent\protected.ps1')
    $acl.SetAccessRuleProtection($true, $true)
    Set-Acl -LiteralPath (Join-Path $root 'provisioning\agent\protected.ps1') -AclObject $acl
    Write-Output 'before:'; Show-Icacls $root

    Lock-BridgeVMFolder $root

    Write-Output 'after:'; Show-Icacls $root
    # The group is left as it was; owner and DACL must be exactly these.
    $sddl = (Get-Acl -LiteralPath $root).Sddl -replace 'G:.*?(?=D:)', ''
    if ($sddl -cne 'O:BAD:PAI(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)(A;OICI;0x1200a9;;;BU)') { throw ('root SDDL is ' + $sddl) }
    foreach ($item in Get-ChildItem -LiteralPath $root -Recurse -Force) {
        $acl = Get-Acl -LiteralPath $item.FullName
        $flags = if ($item.PSIsContainer) { 'ContainerInherit, ObjectInherit' } else { 'None' }
        $want = "S-1-5-18 Allow FullControl $flags True; S-1-5-32-544 Allow FullControl $flags True; S-1-5-32-545 Allow ReadAndExecute, Synchronize $flags True"
        $rules = @($acl.GetAccessRules($true, $true, $sid) | ForEach-Object { '{0} {1} {2} {3} {4}' -f $_.IdentityReference, $_.AccessControlType, $_.FileSystemRights, $_.InheritanceFlags, $_.IsInherited } | Sort-Object) -join '; '
        if ($acl.AreAccessRulesProtected -or $acl.GetOwner($sid).Value -cne 'S-1-5-32-544' -or $rules -cne $want) {
            throw ($item.FullName + ' does not just inherit the locked root: owner ' + $acl.GetOwner($sid).Value + '; ' + $rules)
        }
    }

    $lockedSddl = 'O:BAD:PAI(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)(A;OICI;0x1200a9;;;BU)'
    foreach ($link in '', 'provisioning\agent', 'provisioning\drivers') {
        $root = New-Root
        $target = Join-Path $env:SystemDrive ('bvagent-acl-target-' + [guid]::NewGuid().ToString('N'))
        $script:roots += $target
        $linkPath = if ($link) { Join-Path $root $link } else { $root }
        Move-Item -LiteralPath $linkPath -Destination $target
        cmd /c mklink /J $linkPath $target | Out-Null
        if ($LASTEXITCODE -ne 0) { throw ('mklink /J failed for [' + $link + ']') }
        if ($link) { $script:links += $linkPath } else { $script:links += $root }
        $targetBefore = (Get-Acl -LiteralPath $target).Sddl
        $refused = $null
        try { Lock-BridgeVMFolder $root } catch { $refused = $_.Exception.Message }
        $rootSddl = (Get-Acl -LiteralPath $root).Sddl -replace 'G:.*?(?=D:)', ''
        if ($refused -notlike '*is a reparse point (a junction or link)*' -or (Get-Acl -LiteralPath $target).Sddl -cne $targetBefore -or
            ($link -and $rootSddl -cne $lockedSddl)) {
            throw ('a junction at [' + $link + '] was not refused safely: ' + $refused + '; root ' + $rootSddl)
        }
        cmd /c rmdir $linkPath | Out-Null  # the link only, never the folder it points to
    }
    Write-Output ('C:\BridgeVM lock on ' + $PSVersionTable.PSEdition + ' ' + $PSVersionTable.PSVersion + ': PASS')
} finally {
    foreach ($link in $links) {
        if ((Test-Path -LiteralPath $link) -and ((Get-Item -LiteralPath $link -Force).Attributes -band [IO.FileAttributes]::ReparsePoint)) { cmd /c rmdir $link | Out-Null }
    }
    foreach ($root in $roots) {
        if (Test-Path -LiteralPath $root) { Remove-Item -LiteralPath $root -Recurse -Force }
    }
}
