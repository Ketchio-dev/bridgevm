# BridgeVM guest tools update (the probe side is agent_console/agent_update.rs).
# Runs from its version folder C:\Program Files\BridgeVM\Agent\<id>[.new]:
#   -Arm      run by the old agent: checks every file's SHA-256 and the agent
#             ID, parses every script, checks that only SYSTEM and
#             Administrators can change the files, renames <id>.new to <id>,
#             backs up the agent task and starts the guard task.
#   -Guard    its own scheduled task, so it outlives the agent: registers
#             the agent task from <id>'s bvagent-task.ps1, restarts the
#             agent, and puts the backed-up task back unless -Confirm
#             arrives within 180 s. Its logon trigger finishes the job after
#             a reboot.
#   -Confirm  run by the new agent once the host saw its READY.
param([switch]$Arm, [switch]$Guard, [switch]$Confirm, [string]$Id, [string]$Expect)
$ErrorActionPreference = 'Stop'

$AgentTask = 'BridgeVM Guest Agent'
$GuardTask = 'BridgeVM Guest Tools Update'
$Root = Split-Path -Parent $PSScriptRoot
$State = Join-Path $Root 'update'
$ConfirmSeconds = 180
$RunKey = 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Run'
$RunValue = 'BridgeVMAgent'
$StartupLauncher = Join-Path $env:ProgramData 'Microsoft\Windows\Start Menu\Programs\Startup\bvagent.bat'
# SIDs, not names: Windows ships localized. HvfDisplayDriverInstall's descriptor.
$System = 'S-1-5-18'
$Administrators = 'S-1-5-32-544'
$LockedSddl = 'O:S-1-5-32-544D:PAI(A;OICI;FA;;;S-1-5-18)(A;OICI;FA;;;S-1-5-32-544)'

function Write-StateFile([string]$Name, [string]$Text) {
    [IO.File]::WriteAllText((Join-Path $State $Name), $Text)
}

function Read-StateFile([string]$Name) {
    $path = Join-Path $State $Name
    if (Test-Path -LiteralPath $path) { return ([IO.File]::ReadAllText($path)).Trim() }
    return ''
}

function Write-GuardLog([string]$Message) {
    try {
        $line = (Get-Date).ToString('yyyy-MM-dd HH:mm:ss') + ' ' + $Id + ' ' + $Message + "`r`n"
        [IO.File]::AppendAllText((Join-Path $State 'guard.log'), $line)
    } catch { }
}

function Get-FileSha256([string]$Path) {
    $sha = [System.Security.Cryptography.SHA256]::Create()
    try {
        return [System.BitConverter]::ToString($sha.ComputeHash([IO.File]::ReadAllBytes($Path))).Replace('-', '').ToLowerInvariant()
    } finally { $sha.Dispose() }
}

# The agent ID over "<name> <sha256>" lines, as bvagent.ps1 Get-BvAgentId.
function Get-UpdateId([object[]]$Files) {
    $list = New-Object System.Text.StringBuilder
    foreach ($file in $Files) { [void]$list.Append($file.Name + ' ' + $file.Sha256 + "`n") }
    $sha = [System.Security.Cryptography.SHA256]::Create()
    try {
        $all = $sha.ComputeHash([System.Text.Encoding]::ASCII.GetBytes($list.ToString()))
        return [System.BitConverter]::ToString($all, 0, 8).Replace('-', '').ToLowerInvariant()
    } finally { $sha.Dispose() }
}

# "<name>=<sha256>,..." from the host, in hash order.
function Read-ExpectedFiles([string]$Text) {
    $files = @()
    foreach ($entry in $Text.Split(',')) {
        if ($entry -notmatch '^([A-Za-z0-9][A-Za-z0-9._-]*)=([0-9a-f]{64})$') { throw ('bad file entry: ' + $entry) }
        $files += [pscustomobject]@{ Name = $Matches[1]; Sha256 = $Matches[2] }
    }
    if ($files.Count -eq 0 -or @($files | Select-Object -ExpandProperty Name -Unique).Count -ne $files.Count) { throw 'bad file list' }
    return $files
}

function Test-ReparsePoint([string]$Path) {
    return [bool]([IO.File]::GetAttributes($Path) -band [IO.FileAttributes]::ReparsePoint)
}

# Exactly the descriptor the probe's Prepare set on the root folder.
function Test-Locked([string]$Path) {
    $sid = [Security.Principal.SecurityIdentifier]
    $acl = Get-Acl -LiteralPath $Path
    $aces = $acl.GetAccessRules($true, $true, $sid) | ForEach-Object { '{0} {1} {2} {3} {4}' -f $_.IdentityReference, $_.AccessControlType, $_.FileSystemRights, $_.InheritanceFlags, $_.IsInherited }
    return $acl.AreAccessRulesProtected -and $acl.GetOwner($sid).Value -eq $Administrators -and (@($aces | Sort-Object) -join '; ') -eq 'S-1-5-18 Allow FullControl ContainerInherit, ObjectInherit False; S-1-5-32-544 Allow FullControl ContainerInherit, ObjectInherit False'
}

# An owner may always rewrite the DACL, so the owner must be SYSTEM or
# Administrators too. A file the agent wrote may be owned by its user.
function Set-AdministratorsOwner([string]$Path) {
    $acl = Get-Acl -LiteralPath $Path
    if (@($System, $Administrators) -notcontains $acl.GetOwner([Security.Principal.SecurityIdentifier]).Value) {
        $acl.SetSecurityDescriptorSddlForm('O:BA', [Security.AccessControl.AccessControlSections]::Owner)
        Set-Acl -LiteralPath $Path -AclObject $acl
    }
}

# Owned by and open to SYSTEM and Administrators only: no one else may write.
function Test-AdminOnly([string]$Path) {
    $sid = [Security.Principal.SecurityIdentifier]
    $acl = Get-Acl -LiteralPath $Path
    if (@($System, $Administrators) -notcontains $acl.GetOwner($sid).Value) { return $false }
    foreach ($rule in $acl.GetAccessRules($true, $true, $sid)) {
        if (@($System, $Administrators) -notcontains $rule.IdentityReference.Value) { return $false }
    }
    return $true
}

function Get-TaskAgentPath($Task) {
    if ($null -eq $Task) { return '' }
    foreach ($action in @($Task.Actions)) {
        if ([string]$action.Arguments -match '-File\s+"?([^"]+?\.ps1)"?(\s|$)') { return $Matches[1] }
    }
    return ''
}

# Every running agent, whatever started it, or only those run from $Only.
function Stop-AgentProcesses([string]$Only) {
    foreach ($process in @(Get-CimInstance Win32_Process -Filter "Name='powershell.exe'")) {
        $line = [string]$process.CommandLine
        if ($process.ProcessId -eq $PID -or $line -notmatch 'bvagent\.ps1') { continue }
        if ($Only -and $line.IndexOf($Only, [StringComparison]::OrdinalIgnoreCase) -lt 0) { continue }
        Stop-Process -Id $process.ProcessId -Force -ErrorAction SilentlyContinue
    }
}

function Invoke-Arm {
    $staging = $PSScriptRoot
    if ((Split-Path -Leaf $staging) -cne ($Id + '.new')) { throw 'not run from its staging folder' }
    $files = Read-ExpectedFiles $Expect
    # Nobody but SYSTEM and Administrators can change a file once it passes
    # here, so the hashes checked next stay true.
    if ((Test-ReparsePoint $Root) -or -not (Test-Locked $Root)) { throw ($Root + ' is not locked to SYSTEM and Administrators') }
    foreach ($file in $files) {
        if (-not (Test-Path -LiteralPath (Join-Path $staging $file.Name) -PathType Leaf)) { throw ('missing ' + $file.Name) }
    }
    foreach ($path in @($staging) + @($files | ForEach-Object { Join-Path $staging $_.Name })) {
        if (Test-ReparsePoint $path) { throw ('reparse point: ' + $path) }
        Set-AdministratorsOwner $path
        if (-not (Test-AdminOnly $path)) { throw ('not admin-only: ' + $path) }
    }
    foreach ($file in $files) {
        if ((Get-FileSha256 (Join-Path $staging $file.Name)) -cne $file.Sha256) { throw ('hash ' + $file.Name) }
    }
    if ((Get-UpdateId $files) -cne $Id) { throw 'agent ID does not match the files' }
    foreach ($file in $files) {
        if ($file.Name -notlike '*.ps1') { continue }
        $tokens = $null; $errors = $null
        [void][System.Management.Automation.Language.Parser]::ParseFile((Join-Path $staging $file.Name), [ref]$tokens, [ref]$errors)
        if ($errors.Count -gt 0) {
            $first = $errors[0]
            throw ('parse ' + $file.Name + ' ' + $first.Extent.StartLineNumber + ':' + $first.Extent.StartColumnNumber + ' ' + $first.Message)
        }
    }
    $task = Get-ScheduledTask -TaskName $AgentTask -ErrorAction SilentlyContinue
    if ($null -eq $task) { throw 'no agent task' }
    $running = Get-ScheduledTask -TaskName $GuardTask -ErrorAction SilentlyContinue
    if ($null -ne $running -and [string]$running.State -eq 'Running') { throw 'another update is running' }

    $final = Join-Path $Root $Id
    $current = Get-TaskAgentPath $task
    if (Test-Path -LiteralPath $final) {
        if ($current -and $current.StartsWith($final + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) { throw 'that version is the running agent' }
        Remove-Item -LiteralPath $final -Recurse -Force
    }
    # Antivirus may still hold a just-written file for a moment.
    for ($attempt = 1; ; $attempt++) {
        try { [IO.Directory]::Move($staging, $final); break } catch { if ($attempt -ge 10) { throw }; Start-Sleep -Milliseconds 500 }
    }

    [void][IO.Directory]::CreateDirectory($State)
    foreach ($name in @('pending.txt', 'confirmed.txt', 'result.txt', 'run.txt', 'bvagent.bat')) {
        $path = Join-Path $State $name
        if (Test-Path -LiteralPath $path) { Remove-Item -LiteralPath $path -Force }
    }
    Write-StateFile 'task.xml' (Export-ScheduledTask -TaskName $AgentTask)
    Write-StateFile 'pending.txt' $Id
    Write-GuardLog ('armed; previous agent ' + $current)

    # The guard runs as the agent task's user, elevated like the agent.
    $user = [string]$task.Principal.UserId
    if (-not $user) { $user = [Security.Principal.WindowsIdentity]::GetCurrent().Name }
    $arguments = '-NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -File "' + (Join-Path $final 'bvagent-update.ps1') + '" -Guard -Id ' + $Id
    $action = New-ScheduledTaskAction -Execute 'powershell.exe' -Argument $arguments
    $trigger = New-ScheduledTaskTrigger -AtLogOn -User $user
    $principal = New-ScheduledTaskPrincipal -UserId $user -LogonType Interactive -RunLevel Highest
    $settings = New-ScheduledTaskSettingsSet -ExecutionTimeLimit (New-TimeSpan -Minutes 15) -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries -MultipleInstances IgnoreNew
    Register-ScheduledTask -TaskName $GuardTask -InputObject (New-ScheduledTask -Action $action -Trigger $trigger -Principal $principal -Settings $settings) -Force | Out-Null
    Start-ScheduledTask -TaskName $GuardTask
    return ('BVUPDATE armed id=' + $Id)
}

# Old launchers would start the old agent next to the task at the next logon.
function Disable-LegacyLaunchers {
    $value = (Get-ItemProperty -LiteralPath $RunKey -Name $RunValue -ErrorAction SilentlyContinue).$RunValue
    if ($value) {
        Write-StateFile 'run.txt' $value
        Remove-ItemProperty -LiteralPath $RunKey -Name $RunValue -ErrorAction SilentlyContinue
    }
    if (Test-Path -LiteralPath $StartupLauncher) {
        Move-Item -LiteralPath $StartupLauncher -Destination (Join-Path $State 'bvagent.bat') -Force
    }
}

function Restore-LegacyLaunchers {
    $value = Read-StateFile 'run.txt'
    if ($value) { New-ItemProperty -Path $RunKey -Name $RunValue -Value $value -PropertyType String -Force | Out-Null }
    $saved = Join-Path $State 'bvagent.bat'
    if (Test-Path -LiteralPath $saved) { Move-Item -LiteralPath $saved -Destination $StartupLauncher -Force }
}

function Remove-GuardTask {
    Unregister-ScheduledTask -TaskName $GuardTask -Confirm:$false -ErrorAction SilentlyContinue
}

function Complete-Update {
    Write-StateFile 'result.txt' ('ok ' + $Id)
    Remove-Item -LiteralPath (Join-Path $State 'pending.txt') -Force -ErrorAction SilentlyContinue
    # Keep this version and the one the backed-up task runs; older ones go.
    $keep = @($Id)
    if ([IO.File]::ReadAllText((Join-Path $State 'task.xml')) -match [regex]::Escape($Root) + '[\\/]([0-9a-f]{16})[\\/]') { $keep += $Matches[1] }
    foreach ($folder in @(Get-ChildItem -LiteralPath $Root -Directory)) {
        if ($folder.Name -match '^[0-9a-f]{16}(\.new)?$' -and $keep -notcontains $folder.Name) {
            Remove-Item -LiteralPath $folder.FullName -Recurse -Force -ErrorAction SilentlyContinue
        }
    }
    Write-GuardLog 'confirmed'
    Remove-GuardTask
}

# One restore step: a step that fails must not keep the others from putting
# the old agent back.
function Invoke-RestoreStep([string]$Name, [scriptblock]$Step) {
    try { & $Step } catch { Write-GuardLog ('restore step ' + $Name + ' failed: ' + $_.Exception.Message) }
}

function Restore-PreviousAgent {
    Write-GuardLog 'no confirmation: restoring the previous agent'
    # The backed-up definition goes in before the stop, so no per-minute
    # start of the new definition can bring the new agent back.
    Invoke-RestoreStep 'register' { Register-ScheduledTask -TaskName $AgentTask -Xml ([IO.File]::ReadAllText((Join-Path $State 'task.xml'))) -Force | Out-Null }
    Invoke-RestoreStep 'stop' { Stop-ScheduledTask -TaskName $AgentTask -ErrorAction SilentlyContinue }
    Invoke-RestoreStep 'kill' { Stop-AgentProcesses $PSScriptRoot }
    Invoke-RestoreStep 'launchers' { Restore-LegacyLaunchers }
    Invoke-RestoreStep 'start' { Start-ScheduledTask -TaskName $AgentTask }
    Invoke-RestoreStep 'result' { Write-StateFile 'result.txt' ('rolled-back ' + $Id) }
    Invoke-RestoreStep 'pending' { Remove-Item -LiteralPath (Join-Path $State 'pending.txt') -Force -ErrorAction SilentlyContinue }
    Remove-GuardTask
}

function Invoke-Guard {
    if ((Read-StateFile 'pending.txt') -cne $Id) { Remove-GuardTask; return }
    if ((Read-StateFile 'confirmed.txt') -ceq $Id) { Complete-Update; return }
    Start-Sleep -Seconds 3
    $agent = Join-Path $PSScriptRoot 'bvagent.ps1'
    try {
        $task = Get-ScheduledTask -TaskName $AgentTask -ErrorAction SilentlyContinue
        if ((Get-TaskAgentPath $task) -ne $agent) {
            Write-GuardLog 'switching the agent task'
            Disable-LegacyLaunchers
            # The bundled definition starts the agent at logon and every
            # minute after it exits. It goes in before the old agent stops,
            # so a tick in between starts the new agent, never the old one.
            . (Join-Path $PSScriptRoot 'bvagent-task.ps1')
            $user = [string]$task.Principal.UserId
            if (-not $user) { $user = [Security.Principal.WindowsIdentity]::GetCurrent().Name }
            Register-ScheduledTask -TaskName $AgentTask -Xml (Get-GuestAgentTaskXml $user $agent) -Force | Out-Null
            Stop-ScheduledTask -TaskName $AgentTask -ErrorAction SilentlyContinue
            Stop-AgentProcesses ''
            Start-ScheduledTask -TaskName $AgentTask
        } else {
            Start-ScheduledTask -TaskName $AgentTask -ErrorAction SilentlyContinue
        }
    } catch {
        Write-GuardLog ('switch failed: ' + $_.Exception.Message)
        Restore-PreviousAgent
        return
    }
    # Whatever ends the wait short of a Confirm, an error included, puts the
    # old agent back. A reboot ends the process instead; the logon trigger
    # runs the guard again.
    $settled = $false
    try {
        $deadline = (Get-Date).AddSeconds($ConfirmSeconds)
        while ((Get-Date) -lt $deadline) {
            if ((Read-StateFile 'confirmed.txt') -ceq $Id) { $settled = $true; Complete-Update; return }
            Start-Sleep -Seconds 2
        }
    } finally {
        if (-not $settled) { Restore-PreviousAgent }
    }
}

# The guard decides: Confirm waits for its verdict, so it never answers
# confirmed for an update the guard is already putting back.
function Invoke-Confirm {
    if ((Read-StateFile 'pending.txt') -ceq $Id) {
        Write-StateFile 'confirmed.txt' $Id
    } elseif ((Read-StateFile 'result.txt') -notmatch ('^(ok|rolled-back) ' + $Id + '$')) {
        return ('BVUPDATE nothing-pending id=' + $Id)
    }
    for ($wait = 0; $wait -lt 60; $wait++) {
        $result = Read-StateFile 'result.txt'
        if ($result -ceq ('ok ' + $Id)) { return ('BVUPDATE confirmed id=' + $Id) }
        if ($result -ceq ('rolled-back ' + $Id)) { return ('BVUPDATE rolled-back id=' + $Id) }
        Start-Sleep -Milliseconds 500
    }
    throw 'the guard did not answer within 30 s'
}

if ($Arm -or $Guard -or $Confirm) {
    if ($Id -notmatch '^[0-9a-f]{16}$') { 'BVUPDATE result=failed detail=bad id'; exit 2 }
    try {
        if ($Arm) { Invoke-Arm } elseif ($Confirm) { Invoke-Confirm } else { Invoke-Guard }
    } catch {
        $detail = (($_.Exception.Message -replace '\s+', ' ').Trim())
        if ($Guard) { Write-GuardLog ('guard failed: ' + $detail) }
        'BVUPDATE result=failed detail=' + $detail
        exit 2
    }
}
