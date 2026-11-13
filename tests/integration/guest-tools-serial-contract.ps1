$ErrorActionPreference = 'Stop'
$path = Join-Path $PSScriptRoot '../../scripts/win-assets/bvagent-install.ps1'
$tokens = $null; $errors = $null
$ast = [Management.Automation.Language.Parser]::ParseFile($path, [ref]$tokens, [ref]$errors)
if ($errors.Count) { throw 'guest tools installer parse failed' }
$definition = $ast.Find({ param($node) $node -is [Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -eq 'Test-GuestToolsSerialDevice' }, $true)
if ($null -eq $definition) { throw 'serial detection function is missing' }
. ([scriptblock]::Create($definition.Extent.Text))
# Recorded from the real Windows guest: vioser* SystemDriver query was empty,
# this present PnP device was OK, and the agent's port interface class had two entries.
$present = [pscustomobject]@{
    InstanceId = 'PCI\VEN_1AF4&DEV_1043&SUBSYS_11001AF4&REV_01\3&11583659&0&30'
    Status = 'OK'
    Class = 'System'
    FriendlyName = 'VirtIO Serial Driver'
}
if (-not (Test-GuestToolsSerialDevice -Query { $present })) { throw 'ASSERT: recorded present serial device must allow agent installation' }
if (Test-GuestToolsSerialDevice -Query { @() }) { throw 'ASSERT: no serial device must retain the driverless notice' }
$broken = [pscustomobject]@{ InstanceId = $present.InstanceId; Status = 'Error' }
if (Test-GuestToolsSerialDevice -Query { $broken }) { throw 'ASSERT: a broken serial device must not count as available' }
$unrelated = [pscustomobject]@{ InstanceId = 'PCI\VEN_1AF4&DEV_1041'; Status = 'OK' }
if (Test-GuestToolsSerialDevice -Query { $unrelated }) { throw 'ASSERT: an unrelated device must not count as serial' }
'PASS: 4 guest-tools serial detection assertions; installer parses'
