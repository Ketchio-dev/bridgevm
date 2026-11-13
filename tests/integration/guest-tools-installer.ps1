$ErrorActionPreference = 'Stop'
$root = Split-Path (Split-Path $PSScriptRoot -Parent) -Parent
$path = Join-Path $root 'scripts/win-assets/bvagent-install.ps1'
$t = $null; $e = $null
$ast = [Management.Automation.Language.Parser]::ParseFile($path, [ref]$t, [ref]$e)
if ($e.Count) { throw ($e | Out-String) }
# Exercise ACL validation without requiring a Windows host.
$definition = $ast.Find({param($a) $a -is [Management.Automation.Language.FunctionDefinitionAst] -and $a.Name -eq 'Assert-ToolsFolder'}, $true)
Invoke-Expression $definition.Extent.Text
$folderDefinition = $ast.Find({param($a) $a -is [Management.Automation.Language.FunctionDefinitionAst] -and $a.Name -eq 'New-ToolsFolder'}, $true)
$expectedSddl = 'O:S-1-5-32-544D:PAI(A;OICI;FA;;;S-1-5-18)(A;OICI;FA;;;S-1-5-32-544)(A;OICI;0x1200a9;;;S-1-5-32-545)'
if (-not $folderDefinition.Extent.Text.Contains($expectedSddl)) { throw 'ASSERT: tools folder must grant Users read-execute without write, matching first boot' }
if ($PSVersionTable.PSEdition -eq 'Desktop' -or $IsWindows) {
    $descriptor = New-Object Security.AccessControl.DirectorySecurity
    $descriptor.SetSecurityDescriptorSddlForm($expectedSddl)
    $users = @($descriptor.GetAccessRules($true,$true,[Security.Principal.SecurityIdentifier]) | Where-Object {$_.IdentityReference.Value -eq 'S-1-5-32-545'})
    if ($users.Count -ne 1 -or [int]$users[0].FileSystemRights -ne 0x1200a9) { throw 'ASSERT: parsed Users ACE must be read-execute only' }
} else { Write-Output 'INFO: macOS checks exact SDDL string; Windows parses the descriptor' }
$script:owner='S-1-5-32-544'; $script:rules=@(); $script:attributes=0
function Get-Item { param($LiteralPath,[switch]$Force) [pscustomobject]@{Attributes=$script:attributes} }
function Get-Acl {
    param($LiteralPath)
    $acl=[pscustomobject]@{}
    $acl | Add-Member ScriptMethod GetOwner { param($sid) [pscustomobject]@{Value=$script:owner} }
    $acl | Add-Member ScriptMethod GetAccessRules { param($a,$b,$sid) $script:rules }
    return $acl
}
Assert-ToolsFolder 'safe'
$script:rules=@([pscustomobject]@{PropagationFlags=0;AccessControlType='Allow';IdentityReference=[pscustomobject]@{Value='S-1-5-32-545'};FileSystemRights=0x1200a9})
Assert-ToolsFolder 'safe'
# Recorded after the app's agent update created BridgeVM with inherited Program Files ACLs.
$script:rules=@([pscustomobject]@{PropagationFlags=0;AccessControlType='Allow';IdentityReference=[pscustomobject]@{Value='S-1-5-80-956008885-3418522649-1831038044-1853292631-2271478464'};FileSystemRights=2032127})
$accepted=$true; try { Assert-ToolsFolder 'safe' } catch { $accepted=$false }
if (-not $accepted) { throw 'ASSERT: recorded TrustedInstaller FullControl on an administrator-owned folder is safe' }
$script:rules=@([pscustomobject]@{PropagationFlags=0;AccessControlType='Allow';IdentityReference=[pscustomobject]@{Value='S-1-5-32-545'};FileSystemRights=0x116})
$rejected=$false; try { Assert-ToolsFolder 'unsafe' } catch { $rejected=$true }
if (-not $rejected) { throw 'user-writable folder was accepted' }
$script:rules=@([pscustomobject]@{PropagationFlags=0;AccessControlType='Allow';IdentityReference=[pscustomobject]@{Value='S-1-15-2-1'};FileSystemRights=0x116})
$rejected=$false; try { Assert-ToolsFolder 'unsafe' } catch { $rejected=$true }
if (-not $rejected) { throw 'application-package write access was accepted' }
$script:rules=@(); $script:owner='S-1-5-21-1234'
$rejected=$false; try { Assert-ToolsFolder 'unsafe' } catch { $rejected=$true }
if (-not $rejected) { throw 'user-owned folder was accepted' }
# File publication is exercised with real temp files; only Windows ACL operations are replaced.
$publication = $ast.Find({param($a) $a -is [Management.Automation.Language.FunctionDefinitionAst] -and $a.Name -eq 'Install-ToolsFiles'}, $true)
if ($null -eq $publication) { throw 'ASSERT: tools publication function is required for atomic retry' }
. ([scriptblock]::Create($publication.Extent.Text))
function Assert-ToolsFolder { param($Path) }
function New-ToolsFolder { param($Path) [void][IO.Directory]::CreateDirectory($Path) }
function Copy-ToolsFile { param($Source,$Destination) [IO.File]::WriteAllBytes($Destination,[IO.File]::ReadAllBytes($Source)) }
$work=Join-Path ([IO.Path]::GetTempPath()) ('bv-tools-contract-'+[guid]::NewGuid().ToString('N'))
try {
    $source=Join-Path $root 'scripts/win-assets';$stage=Join-Path $work 'GuestTools.new';$target=Join-Path $work 'GuestTools'
    [void][IO.Directory]::CreateDirectory($stage)
    [IO.File]::WriteAllText((Join-Path $stage 'junk'),'leftover')
    $installed=$true;try { Install-ToolsFiles $source $stage $target } catch { $installed=$false }
    if (-not $installed -or (Test-Path $stage) -or @(Get-ChildItem $target).Count -ne 9 -or (Test-Path (Join-Path $target 'junk'))) { throw 'ASSERT: leftover staging must be replaced and complete tools published' }
    Install-ToolsFiles $source $stage $target
    Remove-Item -LiteralPath (Join-Path $target 'bvagent.ps1')
    $rejected=$false;try { Install-ToolsFiles $source $stage $target } catch { $rejected=$_.Exception.Message -eq 'Unexpected existing tools' }
    if (-not $rejected) { throw 'ASSERT: partial final tools must remain refused, not overwritten' }
} finally { if(Test-Path $work){Remove-Item $work -Recurse -Force} }
'PASS: SDDL/read-execute and 6 ACL cases; staging retry, identical retry and unexpected final refusal'
