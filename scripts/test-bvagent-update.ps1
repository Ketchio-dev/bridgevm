param([string]$UpdateScript = (Join-Path $PSScriptRoot 'win-assets/bvagent-update.ps1'))
$ErrorActionPreference = 'Stop'
# Guest tools updates. The agent reports an ID over its own files in READY
# (bvagent.ps1 Get-BvAgentId); the probe computes the same ID over the app's
# copy (agent_console/agent_update.rs) and updates the guest when they differ,
# through bvagent-update.ps1 (-Arm, -Guard, -Confirm).
#
# The update script runs unchanged against a model of the guest: scheduled
# tasks, processes, the Run key, ACLs and the clock are stand-in functions, so
# nothing is registered, stopped or relocked. On Windows the ACL checks also
# run against real folders. -UpdateScript runs a modified copy instead.
$assets = Join-Path $PSScriptRoot 'win-assets'
$onWindows = $env:OS -eq 'Windows_NT'

function Get-ScriptFunction([string]$Path, [string]$Name) {
    $tokens = $null; $errors = $null
    $ast = [System.Management.Automation.Language.Parser]::ParseFile($Path, [ref]$tokens, [ref]$errors)
    if ($errors.Count -ne 0) { throw ($Path + ' does not parse') }
    $definition = $ast.Find({ param($node)
        $node -is [System.Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -eq $Name
    }, $true)
    if ($null -eq $definition) { throw ($Path + ' has no ' + $Name) }
    return $definition.Extent.Text
}

function Assert-True([bool]$Condition, [string]$Message) {
    if (-not $Condition) { throw ('FAIL: ' + $Message) }
}

$agentFiles = @('bvagent.ps1', 'bvagent-input.ps1', 'bvagent-unicode-input.cs', 'bvagent-key-input.cs', 'bvagent-pointer-input.cs', 'bvagent-window-inventory.ps1', 'bv-window-inventory.cs', 'bvagent-task.ps1', 'bvagent-update.ps1')

# 1. The agent ID: the same fixture and ID as agent_update_tests.rs, with
#    bvagent-update.ps1 missing.
. ([scriptblock]::Create((Get-ScriptFunction (Join-Path $assets 'bvagent.ps1') 'Get-BvAgentId')))
$fixture = Join-Path ([IO.Path]::GetTempPath()) ('bvagent-id-' + [guid]::NewGuid())
[void](New-Item -ItemType Directory -Path $fixture)
try {
    foreach ($name in $agentFiles[0..7]) { [IO.File]::WriteAllText((Join-Path $fixture $name), "fixture $name`r`n") }
    $id = Get-BvAgentId $fixture
    Assert-True ($id -ceq '287c4fd3fa27761d') ('agent ID ' + $id + ' differs from the probe fixture ID')
} finally {
    Remove-Item -LiteralPath $fixture -Recurse -Force
}
$agent = [IO.File]::ReadAllText((Join-Path $assets 'bvagent.ps1'))
Assert-True ($agent.Contains("`$bvReady = Get-BvReady `$PSScriptRoot `$bvAgentId")) 'READY is not built by Get-BvReady'

# 2. A model of the guest.
class FakeSid {
    [string]$Value
    FakeSid([string]$Value) { $this.Value = $Value }
    [string] ToString() { return $this.Value }
}
class FakeAcl {
    [string]$Owner
    [bool]$AreAccessRulesProtected
    [object[]]$Rules
    [object] GetOwner([object]$Type) { return [FakeSid]::new($this.Owner) }
    [object[]] GetAccessRules([bool]$Explicit, [bool]$Inherited, [object]$Type) { return $this.Rules }
    [void] SetSecurityDescriptorSddlForm([string]$Sddl, [object]$Sections) { if ($Sddl -eq 'O:BA') { $this.Owner = 'S-1-5-32-544' } }
}
function New-FakeRule([string]$Sid, [bool]$Inherited, [string]$Rights = 'FullControl') {
    [pscustomobject]@{ IdentityReference = [FakeSid]::new($Sid); AccessControlType = 'Allow'; FileSystemRights = $Rights; InheritanceFlags = 'ContainerInherit, ObjectInherit'; IsInherited = $Inherited }
}
function New-FakeAcl([string]$Owner, [bool]$Protected, [object[]]$Rules) {
    $acl = [FakeAcl]::new(); $acl.Owner = $Owner; $acl.AreAccessRulesProtected = $Protected; $acl.Rules = $Rules; return $acl
}
$lockedRules = @((New-FakeRule 'S-1-5-18' $false), (New-FakeRule 'S-1-5-32-544' $false))
$inheritedRules = @((New-FakeRule 'S-1-5-18' $true), (New-FakeRule 'S-1-5-32-544' $true))

function Get-Acl { [CmdletBinding()] param([string]$LiteralPath)
    $known = $global:BvAcls[$LiteralPath]
    if ($null -eq $known) { $known = New-FakeAcl 'S-1-5-32-544' $false $inheritedRules }
    return (New-FakeAcl $known.Owner $known.AreAccessRulesProtected $known.Rules)
}
function Set-Acl { [CmdletBinding()] param([string]$LiteralPath, $AclObject)
    if (-not $global:BvSetAclFails) { $global:BvAcls[$LiteralPath] = $AclObject }
}
function Get-ScheduledTask { [CmdletBinding()] param([string]$TaskName) $global:BvTasks[$TaskName] }
function Export-ScheduledTask { [CmdletBinding()] param([string]$TaskName) $global:BvTasks[$TaskName].Xml }
function Register-ScheduledTask { [CmdletBinding()] param([string]$TaskName, $InputObject, [string]$Xml, [switch]$Force)
    $global:BvCalls += ('register ' + $TaskName)
    # BvRegisterFails fails the next agent task registrations.
    if ($global:BvRegisterFails -gt 0 -and $TaskName -eq 'BridgeVM Guest Agent') { $global:BvRegisterFails--; throw 'task registration failed' }
    if ($Xml) {
        # Get-ScheduledTask returns the text, not the XML escapes.
        [void]($Xml -match '<Arguments>([^<]*)</Arguments>')
        $global:BvTasks[$TaskName] = New-FakeTask ([System.Net.WebUtility]::HtmlDecode($Matches[1])) $Xml
    } else {
        $global:BvTasks[$TaskName] = New-FakeTask $InputObject.Action.Argument ''
    }
}
function Start-ScheduledTask { [CmdletBinding()] param([string]$TaskName) $global:BvCalls += ('start ' + $TaskName) }
function Stop-ScheduledTask { [CmdletBinding()] param([string]$TaskName) $global:BvCalls += ('stop ' + $TaskName) }
function Unregister-ScheduledTask { [CmdletBinding()] param([string]$TaskName, [switch]$Confirm)
    $global:BvCalls += ('unregister ' + $TaskName); $global:BvTasks.Remove($TaskName)
}
function New-ScheduledTaskAction { [CmdletBinding()] param([string]$Execute, [string]$Argument) @{ Execute = $Execute; Argument = $Argument } }
function New-ScheduledTaskTrigger { [CmdletBinding()] param([switch]$AtLogOn, [string]$User) @{ User = $User } }
function New-ScheduledTaskPrincipal { [CmdletBinding()] param([string]$UserId, [string]$LogonType, [string]$RunLevel) @{ UserId = $UserId; RunLevel = $RunLevel } }
function New-ScheduledTaskSettingsSet { [CmdletBinding()] param($ExecutionTimeLimit, [switch]$AllowStartIfOnBatteries, [switch]$DontStopIfGoingOnBatteries, [string]$MultipleInstances) @{} }
function New-ScheduledTask { [CmdletBinding()] param($Action, $Trigger, $Principal, $Settings) @{ Action = $Action; Trigger = $Trigger; Principal = $Principal } }
function Get-CimInstance { [CmdletBinding()] param([Parameter(Position = 0)][string]$ClassName, [string]$Filter) $global:BvProcesses }
function Stop-Process { [CmdletBinding()] param([int]$Id, [switch]$Force) $global:BvCalls += ('kill ' + $Id) }
function Get-ItemProperty { [CmdletBinding()] param([string]$LiteralPath, [string]$Name)
    if ($global:BvRunValue) { [pscustomobject]@{ $Name = $global:BvRunValue } }
}
function Remove-ItemProperty { [CmdletBinding()] param([string]$LiteralPath, [string]$Name) $global:BvRunValue = $null }
function New-ItemProperty { [CmdletBinding()] param([string]$Path, [string]$Name, [string]$Value, [string]$PropertyType, [switch]$Force) $global:BvRunValue = $Value }
function Get-Date { [CmdletBinding()] param() $global:BvNow }
function Start-Sleep { [CmdletBinding()] param([int]$Seconds, [int]$Milliseconds)
    $global:BvSleeps++
    if ($global:BvThrowAtSleep -and $global:BvSleeps -ge $global:BvThrowAtSleep) { $global:BvThrowAtSleep = 0; throw 'simulated error' }
    $global:BvNow = $global:BvNow.AddSeconds($Seconds).AddMilliseconds($Milliseconds)
    if ($global:BvVerdictAtSleep -and $global:BvSleeps -ge $global:BvVerdictAtSleep) {
        [IO.File]::WriteAllText((Join-Path $global:BvState 'result.txt'), $global:BvVerdict); $global:BvVerdictAtSleep = 0
    }
    if ($global:BvConfirmAtSleep -and $global:BvSleeps -ge $global:BvConfirmAtSleep) {
        [IO.File]::WriteAllText((Join-Path $global:BvState 'confirmed.txt'), $global:BvId)
    }
}
function New-FakeTask([string]$Arguments, [string]$Xml) {
    [pscustomobject]@{ Actions = @([pscustomobject]@{ Arguments = $Arguments }); State = 'Ready'; Principal = [pscustomobject]@{ UserId = 'bridge' }; Xml = $Xml }
}

# 2. READY says update=pending only while update\pending.txt holds the agent's own ID.
. ([scriptblock]::Create((Get-ScriptFunction (Join-Path $assets 'bvagent.ps1') 'Get-BvReady')))
$readyRoot = Join-Path ([IO.Path]::GetTempPath()) ('bvagent-ready-' + [guid]::NewGuid())
$readyDir = Join-Path $readyRoot '0123456789abcdef'
[void](New-Item -ItemType Directory -Path $readyDir -Force)
[void](New-Item -ItemType Directory -Path (Join-Path $readyRoot 'update') -Force)
try {
    Assert-True (-not (Get-BvReady $readyDir '0123456789abcdef').Contains('update=pending')) 'READY says pending without pending.txt'
    [IO.File]::WriteAllText((Join-Path $readyRoot 'update/pending.txt'), "0123456789abcdef`r`n")
    $ready = Get-BvReady $readyDir '0123456789abcdef'
    Assert-True ($ready.EndsWith(' v3-share2 putdata bigfile lssame agent=0123456789abcdef update=pending')) ('READY with its update pending: ' + $ready)
    [IO.File]::WriteAllText((Join-Path $readyRoot 'update/pending.txt'), 'fedcba9876543210')
    Assert-True (-not (Get-BvReady $readyDir '0123456789abcdef').Contains('update=pending')) 'READY says pending for another version'
    Assert-True ((Get-BvReady $readyDir '').EndsWith(' v3-share2 putdata bigfile lssame')) 'READY without an ID'
} finally { Remove-Item -LiteralPath $readyRoot -Recurse -Force }

$base = Join-Path ([IO.Path]::GetTempPath()) ('bvagent-update-' + [guid]::NewGuid())
$savedProgramData = $env:ProgramData
$env:ProgramData = Join-Path $base 'ProgramData'
$startupDir = Join-Path $env:ProgramData 'Microsoft/Windows/Start Menu/Programs/Startup'
$global:BvRegisterFails = 0

# A fresh guest: the old agent's task, a locked root and nothing staged.
function Reset-Guest([string]$Label) {
    $global:BvRoot = Join-Path $base ($Label + '/Agent')
    $global:BvState = Join-Path $global:BvRoot 'update'
    [void](New-Item -ItemType Directory -Path $global:BvRoot -Force)
    [void](New-Item -ItemType Directory -Path $startupDir -Force)
    $global:BvAcls = @{}
    $global:BvAcls[$global:BvRoot] = New-FakeAcl 'S-1-5-32-544' $true $lockedRules
    $global:BvSetAclFails = $false
    $oldArguments = '-NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -File C:\bvagent.ps1'
    $global:BvOldTaskXml = '<Task><Actions><Exec><Arguments>' + $oldArguments + '</Arguments></Exec></Actions></Task>'
    $global:BvTasks = @{ 'BridgeVM Guest Agent' = (New-FakeTask $oldArguments $global:BvOldTaskXml) }
    $global:BvCalls = @()
    $global:BvProcesses = @(
        [pscustomobject]@{ ProcessId = 4100; CommandLine = 'powershell.exe -NoProfile -WindowStyle Hidden -File C:\bvagent.ps1' },
        [pscustomobject]@{ ProcessId = 4200; CommandLine = 'powershell.exe -NoProfile -File C:\other.ps1' })
    $global:BvRunValue = $null
    $global:BvNow = [DateTime]::new(2026, 8, 1, 0, 0, 0)
    $global:BvSleeps = 0
    $global:BvThrowAtSleep = 0
    $global:BvConfirmAtSleep = 0
    $global:BvVerdictAtSleep = 0
    $global:BvVerdict = ''
    $launcher = Join-Path $startupDir 'bvagent.bat'
    if (Test-Path -LiteralPath $launcher) { Remove-Item -LiteralPath $launcher }
}

# What a reboot during the guard's wait leaves: the task switched to the new
# agent, the legacy launchers moved aside, pending.txt.
function Set-RebootedDuringWait([string]$Version) {
    Register-ScheduledTask -TaskName 'BridgeVM Guest Agent' -Xml (Get-GuestAgentTaskXml 'bridge' (Join-Path $Version 'bvagent.ps1')) -Force
    if ($global:BvRunValue) { [IO.File]::WriteAllText((Join-Path $global:BvState 'run.txt'), $global:BvRunValue); $global:BvRunValue = $null }
    $launcher = Join-Path $startupDir 'bvagent.bat'
    if (Test-Path -LiteralPath $launcher) { Move-Item -LiteralPath $launcher -Destination (Join-Path $global:BvState 'bvagent.bat') -Force }
}

function Add-LegacyLaunchers {
    $global:BvRunValue = 'powershell.exe -File C:\bvagent.ps1'
    [IO.File]::WriteAllText((Join-Path $startupDir 'bvagent.bat'), 'start powershell -File C:\bvagent.ps1')
}

. ([scriptblock]::Create((Get-ScriptFunction $UpdateScript 'Get-UpdateId')))
. ([scriptblock]::Create((Get-ScriptFunction (Join-Path $assets 'bvagent-task.ps1') 'Get-GuestAgentTaskXml')))
function Get-ExpectedId([string]$Expect) {
    Get-UpdateId @($Expect.Split(',') | ForEach-Object { $pair = $_.Split('='); [pscustomobject]@{ Name = $pair[0]; Sha256 = $pair[1] } })
}

# Stages the agent files as the probe does: returns @(Id, Expect). A
# 'parse' or 'hash' mutation breaks one file after (hash) or before (parse)
# its hash is taken.
function New-Staged([string]$Mutate = '') {
    $staging = Join-Path $global:BvRoot 'staging'
    [void](New-Item -ItemType Directory -Path $staging -Force)
    $pairs = @()
    foreach ($name in $agentFiles) {
        $path = Join-Path $staging $name
        if ($name -eq 'bvagent-update.ps1') { Copy-Item -LiteralPath $UpdateScript -Destination $path }
        elseif ($name -eq 'bvagent-task.ps1') { Copy-Item -LiteralPath (Join-Path $assets $name) -Destination $path }
        else { [IO.File]::WriteAllText($path, "Write-Output 'fixture $name'`r`n") }
        if ($Mutate -eq 'parse' -and $name -eq 'bvagent-input.ps1') { [IO.File]::AppendAllText($path, "function Broken( {`r`n") }
        $pairs += ($name + '=' + (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant())
    }
    if ($Mutate -eq 'hash') { [IO.File]::AppendAllText((Join-Path $staging 'bvagent-key-input.cs'), "// changed`r`n") }
    $expect = $pairs -join ','
    $id = Get-ExpectedId $expect
    if (-not $Mutate) { Assert-True ((Get-BvAgentId $staging) -ceq $id) 'the agent and the update script compute different IDs' }
    $global:BvId = $id
    Move-Item -LiteralPath $staging -Destination (Join-Path $global:BvRoot ($id + '.new'))
    return @($id, $expect)
}

function Invoke-Update([string]$Folder, [hashtable]$Arguments) {
    $global:LASTEXITCODE = 0
    $out = @(& (Join-Path $Folder 'bvagent-update.ps1') @Arguments)
    return @{ Exit = $global:LASTEXITCODE; Out = ($out -join "`n") }
}

function Invoke-ArmStep([string]$Id, [string]$Expect) {
    Invoke-Update (Join-Path $global:BvRoot ($Id + '.new')) @{ Arm = $true; Id = $Id; Expect = $Expect }
}

function Assert-Refused($Result, [string]$Id, [string]$Detail, [string]$Case) {
    Assert-True ($Result.Exit -eq 2 -and $Result.Out.Contains('BVUPDATE result=failed') -and $Result.Out.Contains($Detail)) ($Case + ': not refused with "' + $Detail + '" (exit ' + $Result.Exit + '): ' + $Result.Out)
    Assert-True (Test-Path -LiteralPath (Join-Path $global:BvRoot ($Id + '.new'))) ($Case + ': the staging folder was renamed')
    Assert-True (-not (Test-Path -LiteralPath (Join-Path $global:BvRoot $Id))) ($Case + ': a version folder exists')
    Assert-True (-not (Test-Path -LiteralPath (Join-Path $global:BvState 'pending.txt'))) ($Case + ': pending.txt was written')
    Assert-True ($global:BvCalls.Count -eq 0) ($Case + ': tasks were touched: ' + ($global:BvCalls -join ', '))
}

function Read-State([string]$Name) {
    $path = Join-Path $global:BvState $Name
    if (Test-Path -LiteralPath $path) { return (Get-Content -LiteralPath $path -Raw).Trim() }
    return ''
}

function Assert-RolledBack([string]$Case, [string]$Version) {
    Assert-True ($global:BvTasks['BridgeVM Guest Agent'].Actions[0].Arguments.Contains('-File C:\bvagent.ps1')) ($Case + ': the old task was not restored')
    Assert-True ((Read-State 'result.txt') -ceq ('rolled-back ' + $global:BvId)) ($Case + ': result.txt')
    Assert-True ($global:BvTasks['BridgeVM Guest Agent'].Xml -ceq $global:BvOldTaskXml) ($Case + ': the restored task is not the backed-up definition')
    Assert-True ($global:BvCalls -contains 'start BridgeVM Guest Agent') ($Case + ': the old agent was not started')
    Assert-True ($global:BvCalls[-1] -ceq 'unregister BridgeVM Guest Tools Update') ($Case + ': the guard stayed registered')
    Assert-True ($global:BvRunValue -ceq 'powershell.exe -File C:\bvagent.ps1' -and (Test-Path -LiteralPath (Join-Path $startupDir 'bvagent.bat'))) ($Case + ': the legacy launchers were not restored')
    Assert-True (Test-Path -LiteralPath $Version) ($Case + ': the new version folder was deleted')
}

try {
    # 3. Arm refuses before it renames, backs up or registers anything.
    $cases = @(
        @{ Name = 'unprotected root (wrong SDDL)'; Detail = 'is not locked'; Setup = { $global:BvAcls[$global:BvRoot] = New-FakeAcl 'S-1-5-32-544' $false $lockedRules } },
        @{ Name = 'extra ACE on the root'; Detail = 'is not locked'; Setup = { $global:BvAcls[$global:BvRoot] = New-FakeAcl 'S-1-5-32-544' $true ($lockedRules + (New-FakeRule 'S-1-5-32-545' $false 'ReadAndExecute')) } },
        @{ Name = 'root owned by the user'; Detail = 'is not locked'; Setup = { $global:BvAcls[$global:BvRoot] = New-FakeAcl 'S-1-5-21-1-2-3-1001' $true $lockedRules } },
        @{ Name = 'Users may write a file'; Detail = 'not admin-only'; Setup = { param($s) $global:BvAcls[(Join-Path $s 'bvagent.ps1')] = New-FakeAcl 'S-1-5-32-544' $false ($inheritedRules + (New-FakeRule 'S-1-5-32-545' $false 'Write')) } },
        @{ Name = 'a file owner that cannot be changed'; Detail = 'not admin-only'; Setup = { param($s) $global:BvAcls[(Join-Path $s 'bvagent-task.ps1')] = New-FakeAcl 'S-1-5-21-1-2-3-1001' $false $inheritedRules; $global:BvSetAclFails = $true } },
        @{ Name = 'hash mismatch'; Detail = 'hash bvagent-key-input.cs'; Mutate = 'hash' },
        @{ Name = 'ID mismatch'; Detail = 'agent ID does not match'; WrongId = $true },
        @{ Name = 'parse failure'; Detail = 'parse bvagent-input.ps1 2:'; Mutate = 'parse' }
    )
    foreach ($case in $cases) {
        Reset-Guest ('arm-' + [guid]::NewGuid().ToString('N').Substring(0, 8))
        $staged = New-Staged ([string]$case.Mutate)
        $id = $staged[0]
        if ($case.Setup) { & $case.Setup (Join-Path $global:BvRoot ($id + '.new')) }
        if ($case.WrongId) {
            # Well-formed, but not the ID of these files.
            Move-Item -LiteralPath (Join-Path $global:BvRoot ($id + '.new')) -Destination (Join-Path $global:BvRoot '0123456789abcdef.new')
            $id = '0123456789abcdef'
        }
        Assert-Refused (Invoke-ArmStep $id $staged[1]) $id $case.Detail $case.Name
    }

    # 4. Arm, the guard's switch, Confirm.
    Reset-Guest 'switch'
    Add-LegacyLaunchers
    $staged = New-Staged; $id = $staged[0]
    $userFile = Join-Path (Join-Path $global:BvRoot ($id + '.new')) 'bvagent-window-inventory.ps1'
    $global:BvAcls[$userFile] = New-FakeAcl 'S-1-5-21-1-2-3-1001' $false $inheritedRules
    $armed = Invoke-ArmStep $id $staged[1]
    $version = Join-Path $global:BvRoot $id
    Assert-True ($armed.Exit -eq 0 -and $armed.Out -ceq ('BVUPDATE armed id=' + $id)) ('Arm failed: ' + $armed.Out)
    Assert-True ((Test-Path -LiteralPath $version) -and -not (Test-Path -LiteralPath ($version + '.new'))) 'Arm did not rename the staging folder'
    Assert-True ($global:BvAcls[$userFile].Owner -ceq 'S-1-5-32-544') 'a file owned by the user was not given to Administrators'
    Assert-True ((Read-State 'pending.txt') -ceq $id) 'pending.txt'
    Assert-True ((Read-State 'task.xml').Contains('-File C:\bvagent.ps1')) 'the old task was not backed up'
    Assert-True (($global:BvCalls -join ',') -ceq 'register BridgeVM Guest Tools Update,start BridgeVM Guest Tools Update') ('Arm calls: ' + ($global:BvCalls -join ', '))
    Assert-True ($global:BvTasks['BridgeVM Guest Agent'].Actions[0].Arguments.Contains('C:\bvagent.ps1')) 'Arm changed the agent task'
    Assert-True ($global:BvTasks['BridgeVM Guest Tools Update'].Actions[0].Arguments.Contains('-Guard -Id ' + $id)) 'the guard task action'
    $older = Join-Path $global:BvRoot 'fedcba9876543210'
    [void](New-Item -ItemType Directory -Path $older)
    $global:BvCalls = @()
    $global:BvConfirmAtSleep = 5
    $guard = Invoke-Update $version @{ Guard = $true; Id = $id }
    $calls = $global:BvCalls -join ','
    Assert-True ($guard.Exit -eq 0) ('the guard failed: ' + $guard.Out)
    # The new definition goes in before the old agent stops, so a per-minute start in between runs the new agent.
    Assert-True ($calls -ceq 'register BridgeVM Guest Agent,stop BridgeVM Guest Agent,kill 4100,start BridgeVM Guest Agent,unregister BridgeVM Guest Tools Update') ('guard calls: ' + $calls)
    $switched = $global:BvTasks['BridgeVM Guest Agent'].Xml
    Assert-True ($switched -ceq (Get-GuestAgentTaskXml 'bridge' (Join-Path $version 'bvagent.ps1'))) 'the agent task is not the bundled bvagent-task.ps1 definition'
    Assert-True ($switched.Contains('<Interval>PT1M</Interval>')) 'the updated agent task has no per-minute start'
    Assert-True ((Read-State 'result.txt') -ceq ('ok ' + $id)) 'result.txt after Confirm'
    Assert-True (-not (Test-Path -LiteralPath (Join-Path $global:BvState 'pending.txt'))) 'pending.txt left after Confirm'
    Assert-True (-not $global:BvRunValue -and -not (Test-Path -LiteralPath (Join-Path $startupDir 'bvagent.bat'))) 'the legacy launchers were left'
    Assert-True ((Test-Path -LiteralPath $version) -and -not (Test-Path -LiteralPath $older)) 'older versions were not removed'
    $confirmed = Invoke-Update $version @{ Confirm = $true; Id = $id }
    Assert-True ($confirmed.Exit -eq 0 -and $confirmed.Out -ceq ('BVUPDATE confirmed id=' + $id)) ('Confirm after completion: ' + $confirmed.Out)
    $none = Invoke-Update $version @{ Confirm = $true; Id = '0000000000000000' }
    Assert-True ($none.Exit -eq 0 -and $none.Out -ceq 'BVUPDATE nothing-pending id=0000000000000000') ('Confirm of an ID that is not pending: ' + $none.Out)
    Reset-Guest 'confirm'
    $staged = New-Staged; $id = $staged[0]
    [void](Invoke-ArmStep $id $staged[1])
    # Confirm answers the guard's verdict, never ahead of it: a rollback under way is reported as one.
    foreach ($case in @(
        @{ Verdict = ('ok ' + $id); Out = ('BVUPDATE confirmed id=' + $id); Exit = 0 },
        @{ Verdict = ('rolled-back ' + $id); Out = ('BVUPDATE rolled-back id=' + $id); Exit = 0 },
        @{ Verdict = ''; Out = 'the guard did not answer'; Exit = 2 })) {
        foreach ($name in @('result.txt', 'confirmed.txt')) { Remove-Item -LiteralPath (Join-Path $global:BvState $name) -Force -ErrorAction SilentlyContinue }
        [IO.File]::WriteAllText((Join-Path $global:BvState 'pending.txt'), $id)
        $global:BvSleeps = 0; $global:BvVerdict = $case.Verdict; $global:BvVerdictAtSleep = $(if ($case.Verdict) { 3 } else { 0 })
        $confirmed = Invoke-Update (Join-Path $global:BvRoot $id) @{ Confirm = $true; Id = $id }
        Assert-True ($confirmed.Exit -eq $case.Exit -and $confirmed.Out.Contains($case.Out) -and (Read-State 'confirmed.txt') -ceq $id) ('Confirm with the verdict "' + $case.Verdict + '": exit ' + $confirmed.Exit + ' ' + $confirmed.Out)
    }

    # 5. Rollback when no Confirm arrives within 180 s.
    Reset-Guest 'no-confirm'
    Add-LegacyLaunchers
    $staged = New-Staged; $id = $staged[0]
    [void](Invoke-ArmStep $id $staged[1])
    $version = Join-Path $global:BvRoot $id
    $global:BvCalls = @()
    $start = $global:BvNow
    [void](Invoke-Update $version @{ Guard = $true; Id = $id })
    Assert-True (($global:BvNow - $start).TotalSeconds -ge 180) 'rolled back before 180 s'
    # The backed-up definition goes in before the stop, so no per-minute start of the new one follows it.
    $calls = $global:BvCalls -join ','
    Assert-True ($calls -ceq 'register BridgeVM Guest Agent,stop BridgeVM Guest Agent,kill 4100,start BridgeVM Guest Agent,register BridgeVM Guest Agent,stop BridgeVM Guest Agent,start BridgeVM Guest Agent,unregister BridgeVM Guest Tools Update') ('rollback calls: ' + $calls)
    Assert-RolledBack 'no Confirm' $version

    # A failed switch rolls back at once.
    Reset-Guest 'switch-fails'
    Add-LegacyLaunchers
    $staged = New-Staged; $id = $staged[0]
    [void](Invoke-ArmStep $id $staged[1])
    $global:BvRegisterFails = 1
    $start = $global:BvNow
    try { [void](Invoke-Update (Join-Path $global:BvRoot $id) @{ Guard = $true; Id = $id }) } finally { $global:BvRegisterFails = 0 }
    Assert-True (($global:BvNow - $start).TotalSeconds -lt 10) 'a failed switch waited for Confirm'
    Assert-RolledBack 'failed switch' (Join-Path $global:BvRoot $id)

    # An error during the wait still puts the old agent back.
    Reset-Guest 'wait-error'
    Add-LegacyLaunchers
    $staged = New-Staged; $id = $staged[0]
    [void](Invoke-ArmStep $id $staged[1])
    $global:BvThrowAtSleep = 4
    [void](Invoke-Update (Join-Path $global:BvRoot $id) @{ Guard = $true; Id = $id })
    Assert-RolledBack 'error during the wait' (Join-Path $global:BvRoot $id)

    # A restore step that fails does not keep the others from running.
    Reset-Guest 'restore-step'
    Add-LegacyLaunchers
    $staged = New-Staged; $id = $staged[0]
    [void](Invoke-ArmStep $id $staged[1])
    $global:BvRegisterFails = 2
    try { [void](Invoke-Update (Join-Path $global:BvRoot $id) @{ Guard = $true; Id = $id }) } finally { $global:BvRegisterFails = 0 }
    Assert-RolledBack 'a failing restore step' (Join-Path $global:BvRoot $id)
    Assert-True ((Read-State 'guard.log').Contains('restore step register failed')) 'the failed restore step was not logged'

    # A reboot during the wait ends the guard's process: the logon run finds the
    # switched task, waits again, then rolls back.
    Reset-Guest 'reboot'
    Add-LegacyLaunchers
    $staged = New-Staged; $id = $staged[0]
    [void](Invoke-ArmStep $id $staged[1])
    $version = Join-Path $global:BvRoot $id
    Set-RebootedDuringWait $version
    $global:BvCalls = @()
    $start = $global:BvNow
    [void](Invoke-Update $version @{ Guard = $true; Id = $id })
    $calls = $global:BvCalls -join ','
    Assert-True ($calls -ceq 'start BridgeVM Guest Agent,register BridgeVM Guest Agent,stop BridgeVM Guest Agent,start BridgeVM Guest Agent,unregister BridgeVM Guest Tools Update') ('the logon run switched again: ' + $calls)
    Assert-True (($global:BvNow - $start).TotalSeconds -ge 180) 'the logon run did not wait for Confirm'
    Assert-RolledBack 'reboot' $version

    # A reboot, then Confirm before the logon run: the update completes.
    Reset-Guest 'reboot-confirmed'
    $staged = New-Staged; $id = $staged[0]
    [void](Invoke-ArmStep $id $staged[1])
    $version = Join-Path $global:BvRoot $id
    Set-RebootedDuringWait $version
    [IO.File]::WriteAllText((Join-Path $global:BvState 'confirmed.txt'), $id)
    [void](Invoke-Update $version @{ Guard = $true; Id = $id })
    Assert-True ((Read-State 'result.txt') -ceq ('ok ' + $id)) 'not confirmed after a reboot'
    Assert-True ($global:BvTasks['BridgeVM Guest Agent'].Actions[0].Arguments.Contains($version)) 'the new task was replaced'

    # 6. Windows: the same ACL checks against real folders.
    if ($onWindows) {
        Remove-Item function:Get-Acl, function:Set-Acl
        foreach ($name in @('Test-Locked', 'Test-AdminOnly', 'Set-AdministratorsOwner')) { . ([scriptblock]::Create((Get-ScriptFunction $UpdateScript $name))) }
        $System = 'S-1-5-18'; $Administrators = 'S-1-5-32-544'
        $real = Join-Path $base 'real-acl'
        [void](New-Item -ItemType Directory -Path $real)
        $acl = Get-Acl -LiteralPath $real
        $acl.SetSecurityDescriptorSddlForm('O:S-1-5-32-544D:PAI(A;OICI;FA;;;S-1-5-18)(A;OICI;FA;;;S-1-5-32-544)')
        Set-Acl -LiteralPath $real -AclObject $acl
        Assert-True (Test-Locked $real) 'a folder with the locked descriptor is not locked'
        $file = Join-Path $real 'agent.ps1'
        [IO.File]::WriteAllText($file, 'x')
        Assert-True (Test-AdminOnly $file) 'a file inheriting the lock is not admin-only'
        $acl = Get-Acl -LiteralPath $file; $acl.SetOwner([Security.Principal.WindowsIdentity]::GetCurrent().User); Set-Acl -LiteralPath $file -AclObject $acl
        Assert-True (-not (Test-AdminOnly $file)) 'a file owned by the user is admin-only'
        Set-AdministratorsOwner $file
        Assert-True (Test-AdminOnly $file) 'Set-AdministratorsOwner did not give the file to Administrators'
        $users = New-Object Security.Principal.SecurityIdentifier 'S-1-5-32-545'
        $acl = Get-Acl -LiteralPath $file
        $acl.AddAccessRule((New-Object Security.AccessControl.FileSystemAccessRule($users, 'Write', 'Allow')))
        Set-Acl -LiteralPath $file -AclObject $acl
        Assert-True (-not (Test-AdminOnly $file)) 'a file Users may write is admin-only'
        $plain = Join-Path $base 'plain'
        [void](New-Item -ItemType Directory -Path $plain)
        Assert-True (-not (Test-Locked $plain)) 'a folder with inherited permissions is locked'
        $acl = Get-Acl -LiteralPath $real
        $acl.AddAccessRule((New-Object Security.AccessControl.FileSystemAccessRule($users, 'ReadAndExecute', 'ContainerInherit, ObjectInherit', 'None', 'Allow')))
        Set-Acl -LiteralPath $real -AclObject $acl
        Assert-True (-not (Test-Locked $real)) 'a locked folder with a Users ACE is locked'
        Write-Output 'PASS: real ACL checks'

        # The bvinject installer registers its task with schtasks /Create /XML
        # (bvagent-install-service.c, the same XML as Get-GuestAgentTaskXml): Arm's
        # Export-ScheduledTask backup and the guard's Register-ScheduledTask -Xml
        # restore give it back unchanged, per-minute start included.
        Remove-Item function:Register-ScheduledTask, function:Export-ScheduledTask, function:Get-ScheduledTask, function:Unregister-ScheduledTask
        function Get-TaskCore([string]$Xml) {
            $nodes = @(([xml]$Xml).Task.ChildNodes | Where-Object { $_.LocalName -ne 'RegistrationInfo' } | ForEach-Object { $_.OuterXml })
            return (($nodes -join '') -replace '\s+', '')
        }
        $name = 'BridgeVM Update Contract ' + [guid]::NewGuid().ToString('N')
        $xmlPath = Join-Path $base 'installer-task.xml'
        $identity = [Security.Principal.WindowsIdentity]::GetCurrent().Name
        [IO.File]::WriteAllText($xmlPath, (Get-GuestAgentTaskXml $identity 'C:\BridgeVM Contract\bvagent.ps1'), [Text.Encoding]::Unicode)
        try {
            $ErrorActionPreference = 'Continue'
            $created = & schtasks.exe /Create /TN $name /XML $xmlPath /F 2>&1
            $code = $LASTEXITCODE
            $ErrorActionPreference = 'Stop'
            Assert-True ($code -eq 0) ('schtasks /Create /XML failed: ' + ($created -join ' '))
            $backup = [string](Export-ScheduledTask -TaskName $name)
            Register-ScheduledTask -TaskName $name -Xml $backup -Force | Out-Null
            $restored = [string](Export-ScheduledTask -TaskName $name)
            Assert-True ((Get-TaskCore $backup) -ceq (Get-TaskCore $restored)) 'the restored installer task differs from its backup'
            Assert-True (([xml]$restored).Task.Triggers.TimeTrigger.Repetition.Interval -eq 'PT1M') 'the restored installer task lost its per-minute start'
        } finally {
            $ErrorActionPreference = 'Continue'
            & schtasks.exe /Delete /TN $name /F 2>&1 | Out-Null
            $ErrorActionPreference = 'Stop'
        }
        Write-Output 'PASS: schtasks /XML task round trip'
    }
} finally {
    $env:ProgramData = $savedProgramData
    Remove-Item -LiteralPath $base -Recurse -Force -ErrorAction SilentlyContinue
}
Write-Output 'PASS: guest tools update (ID, Arm refusals, switch, Confirm, rollback, reboot)'
