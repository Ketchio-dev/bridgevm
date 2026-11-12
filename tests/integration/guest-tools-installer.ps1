$ErrorActionPreference = 'Stop'
$root = Split-Path (Split-Path $PSScriptRoot -Parent) -Parent
$path = Join-Path $root 'scripts/win-assets/bvagent-install.ps1'
$t = $null; $e = $null
$ast = [Management.Automation.Language.Parser]::ParseFile($path, [ref]$t, [ref]$e)
if ($e.Count) { throw ($e | Out-String) }
# Exercise ACL validation without requiring a Windows host.
$definition = $ast.Find({param($a) $a -is [Management.Automation.Language.FunctionDefinitionAst] -and $a.Name -eq 'Assert-ToolsFolder'}, $true)
Invoke-Expression $definition.Extent.Text
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
$script:rules=@([pscustomobject]@{PropagationFlags=0;AccessControlType='Allow';IdentityReference=[pscustomobject]@{Value='S-1-5-32-545'};FileSystemRights=0x116})
$rejected=$false; try { Assert-ToolsFolder 'unsafe' } catch { $rejected=$true }
if (-not $rejected) { throw 'user-writable folder was accepted' }
$script:rules=@(); $script:owner='S-1-5-21-1234'
$rejected=$false; try { Assert-ToolsFolder 'unsafe' } catch { $rejected=$true }
if (-not $rejected) { throw 'user-owned folder was accepted' }
'PASS: installer parse and safe/user-writable/user-owned ACL behavior'
