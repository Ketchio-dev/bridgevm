# The guest agent's scheduled task, for first logon and for re-registering an existing VM.
#
# Task Scheduler restarts a task only when it fails to start. A killed or crashed agent counts
# as a completed run (event 201), and a logon trigger fires once per logon, so a second trigger
# fires every minute. While the agent runs, IgnoreNew makes a tick start nothing; after any exit
# the next tick starts it again. A start that races a dying agent exits on the singleton mutex,
# and the next tick retries. The principal is the user's interactive token, so a tick with
# nobody logged on runs nothing. ExecutionTimeLimit PT0S: Task Scheduler never stops the agent.
#
# Registering replaces the whole definition (triggers, settings, principal and action), so
# running Start-VerifiedGuestAgentTask again on a VM that already has the task brings it up to
# date. bvagent-install-service.c registers the same XML for the bvinject images' "bridge" user.
function Get-GuestAgentTaskXml([string]$UserId, [string]$AgentPath) {
    $user = [System.Security.SecurityElement]::Escape($UserId)
    $arguments = [System.Security.SecurityElement]::Escape('-NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -File "' + $AgentPath + '"')
    $lines = @(
        "<?xml version='1.0' encoding='UTF-16'?>",
        "<Task version='1.2' xmlns='http://schemas.microsoft.com/windows/2004/02/mit/task'>",
        "  <Triggers>",
        "    <LogonTrigger>",
        "      <Enabled>true</Enabled>",
        "      <UserId>$user</UserId>",
        "    </LogonTrigger>",
        "    <TimeTrigger>",
        "      <Repetition>",
        "        <Interval>PT1M</Interval>",
        "        <StopAtDurationEnd>false</StopAtDurationEnd>",
        "      </Repetition>",
        "      <StartBoundary>2020-01-01T00:00:00</StartBoundary>",
        "      <Enabled>true</Enabled>",
        "    </TimeTrigger>",
        "  </Triggers>",
        "  <Principals>",
        "    <Principal id='Author'>",
        "      <UserId>$user</UserId>",
        "      <LogonType>InteractiveToken</LogonType>",
        "      <RunLevel>HighestAvailable</RunLevel>",
        "    </Principal>",
        "  </Principals>",
        "  <Settings>",
        "    <MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>",
        "    <ExecutionTimeLimit>PT0S</ExecutionTimeLimit>",
        "    <RestartOnFailure>",
        "      <Interval>PT1M</Interval>",
        "      <Count>3</Count>",
        "    </RestartOnFailure>",
        "  </Settings>",
        "  <Actions Context='Author'>",
        "    <Exec>",
        "      <Command>powershell.exe</Command>",
        "      <Arguments>$arguments</Arguments>",
        "    </Exec>",
        "  </Actions>",
        "</Task>"
    )
    return ($lines -join "`r`n") + "`r`n"
}

function Start-VerifiedGuestAgentTask([string]$TaskName, [string]$AgentPath) {
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent().Name
    if ([string]::IsNullOrWhiteSpace($identity) -or $identity.EndsWith('$')) {
        throw 'first-logon interactive user identity is unavailable'
    }
    Register-ScheduledTask -TaskName $TaskName -Xml (Get-GuestAgentTaskXml $identity $AgentPath) -Force | Out-Null
    Start-ScheduledTask -TaskName $TaskName
    return $identity
}
