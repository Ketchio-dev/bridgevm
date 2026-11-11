$ErrorActionPreference = 'Stop'
# The guest agent's scheduled task. Get-GuestAgentTaskXml (bvagent-task.ps1, product VMs) and the
# XML compiled into bvagent-install-service.c (bvinject images) must be one definition: a logon
# trigger plus an every-minute trigger that starts the agent again after it exits, the user's
# interactive token, IgnoreNew, no execution time limit, and running on battery (Task Scheduler's
# defaults would keep the agent from starting on battery and stop it when the Mac is unplugged).
# On Windows, Start-VerifiedGuestAgentTask
# also registers it with Task Scheduler twice, the second time over the task the first left (the
# re-registration an existing VM gets), and reads the definition back each time.
$root = Split-Path (Split-Path $PSScriptRoot -Parent) -Parent
. (Join-Path $root 'scripts/win-assets/bvagent-task.ps1')
$script:checks = 0

function Read-Task([string]$Text) {
    $doc = New-Object System.Xml.XmlDocument
    $doc.LoadXml($Text)
    $ns = New-Object System.Xml.XmlNamespaceManager($doc.NameTable)
    $ns.AddNamespace('t', 'http://schemas.microsoft.com/windows/2004/02/mit/task')
    return @{ Doc = $doc; Ns = $ns }
}

# The comma keeps a single node an array: PowerShell would unroll it on the way out.
function Get-Nodes($Task, [string]$Path) { return ,@($Task.Doc.SelectNodes($Path, $Task.Ns)) }

# Task Scheduler may leave a default value out when it writes a definition back.
function Get-Text($Task, [string]$Path, [string]$Default) {
    $nodes = Get-Nodes $Task $Path
    if ($nodes.Count -eq 0 -and $PSBoundParameters.ContainsKey('Default')) { return $Default }
    if ($nodes.Count -ne 1) { throw "expected one $Path, found $($nodes.Count)" }
    return $nodes[0].InnerText
}

function Assert-Equal($Actual, $Expected, [string]$What) {
    if ($Actual -cne $Expected) { throw "$What is '$Actual', expected '$Expected'" }
    $script:checks++
}

# Read back from Task Scheduler, a user may come back as a SID.
function Assert-User([string]$Actual, [string]$Expected, [string]$What) {
    if ($Actual -ine $Expected) {
        $sid = { param($name) if ($name -match '^S-1-') { $name } else { (New-Object System.Security.Principal.NTAccount($name)).Translate([System.Security.Principal.SecurityIdentifier]).Value } }
        if ((& $sid $Actual) -ne (& $sid $Expected)) { throw "$What is '$Actual', expected '$Expected'" }
    }
    $script:checks++
}

function Assert-AgentTask($Task, [string]$User, [string]$AgentPath, [string]$Label) {
    $triggers = Get-Nodes $Task '/t:Task/t:Triggers/*'
    Assert-Equal (($triggers | ForEach-Object { $_.LocalName }) -join ',') 'LogonTrigger,TimeTrigger' "$Label triggers"
    Assert-Equal (Get-Text $Task '/t:Task/t:Triggers/t:LogonTrigger/t:Enabled' 'true') 'true' "$Label logon trigger Enabled"
    Assert-User (Get-Text $Task '/t:Task/t:Triggers/t:LogonTrigger/t:UserId') $User "$Label logon trigger UserId"
    $tick = '/t:Task/t:Triggers/t:TimeTrigger'
    Assert-Equal (Get-Text $Task "$tick/t:Enabled" 'true') 'true' "$Label minute trigger Enabled"
    Assert-Equal (Get-Text $Task "$tick/t:Repetition/t:Interval") 'PT1M' "$Label minute trigger Interval"
    Assert-Equal (Get-Text $Task "$tick/t:Repetition/t:StopAtDurationEnd" 'false') 'false' "$Label minute trigger StopAtDurationEnd"
    # No Duration and no EndBoundary: it repeats for as long as the task exists.
    Assert-Equal (Get-Nodes $Task "$tick/t:Repetition/t:Duration | $tick/t:EndBoundary").Count 0 "$Label minute trigger end"
    $start = [datetime]::Parse((Get-Text $Task "$tick/t:StartBoundary"), [Globalization.CultureInfo]::InvariantCulture)
    if ($start -gt [datetime]'2021-01-01') { throw "$Label minute trigger starts at $start, not long before any VM exists" }
    $script:checks++
    # The interactive user's own token: a tick with nobody logged on runs nothing.
    $principal = '/t:Task/t:Principals/t:Principal'
    Assert-Equal (Get-Nodes $Task $principal).Count 1 "$Label principals"
    Assert-User (Get-Text $Task "$principal/t:UserId") $User "$Label principal UserId"
    Assert-Equal (Get-Text $Task "$principal/t:LogonType") 'InteractiveToken' "$Label principal LogonType"
    Assert-Equal (Get-Text $Task "$principal/t:RunLevel") 'HighestAvailable' "$Label principal RunLevel"
    $id = (Get-Nodes $Task $principal)[0].GetAttribute('id')
    Assert-Equal (Get-Nodes $Task '/t:Task/t:Actions')[0].GetAttribute('Context') $id "$Label action Context"
    $settings = '/t:Task/t:Settings'
    Assert-Equal (Get-Text $Task "$settings/t:MultipleInstancesPolicy" 'IgnoreNew') 'IgnoreNew' "$Label MultipleInstances"
    # Both default to true when the definition leaves them out.
    Assert-Equal (Get-Text $Task "$settings/t:DisallowStartIfOnBatteries" 'true') 'false' "$Label DisallowStartIfOnBatteries"
    Assert-Equal (Get-Text $Task "$settings/t:StopIfGoingOnBatteries" 'true') 'false' "$Label StopIfGoingOnBatteries"
    Assert-Equal (Get-Text $Task "$settings/t:ExecutionTimeLimit") 'PT0S' "$Label ExecutionTimeLimit"
    Assert-Equal (Get-Text $Task "$settings/t:RestartOnFailure/t:Count") '3' "$Label RestartCount"
    Assert-Equal (Get-Text $Task "$settings/t:RestartOnFailure/t:Interval") 'PT1M' "$Label RestartInterval"
    Assert-Equal (Get-Text $Task "$settings/t:Enabled" 'true') 'true' "$Label Enabled"
    Assert-Equal (Get-Nodes $Task '/t:Task/t:Actions/t:Exec').Count 1 "$Label actions"
    Assert-Equal (Get-Text $Task '/t:Task/t:Actions/t:Exec/t:Command') 'powershell.exe' "$Label Command"
    Assert-Equal (Get-Text $Task '/t:Task/t:Actions/t:Exec/t:Arguments') ('-NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -File "' + $AgentPath + '"') "$Label Arguments"
}

# The product definition, with a user and a path that need XML escaping.
$user = "BRIDGEVM\o'brien & co"
$agent = 'C:\Program Files\B&V <x>\bvagent.ps1'
Assert-AgentTask (Read-Task (Get-GuestAgentTaskXml $user $agent)) $user $agent 'Get-GuestAgentTaskXml'

# The bvinject installer registers the same XML for the image's user and agent path.
$service = [IO.File]::ReadAllText((Join-Path $root 'scripts/win-assets/bvagent-install-service.c'))
$block = [regex]::Match($service, '(?s)/\* BVAGENT_TASK_XML_BEGIN \*/(?<body>.*?)/\* BVAGENT_TASK_XML_END \*/')
if (-not $block.Success) { throw 'bvagent-install-service.c has no BVAGENT_TASK_XML block' }
$literal = -join ([regex]::Matches($block.Groups['body'].Value, 'L"((?:[^"\\]|\\.)*)"') | ForEach-Object { $_.Groups[1].Value })
$unescape = [System.Text.RegularExpressions.MatchEvaluator] { param($m) switch -CaseSensitive ($m.Groups[1].Value) { 'r' { "`r" } 'n' { "`n" } default { $m.Groups[1].Value } } }
$serviceXml = [regex]::Replace($literal, '\\(.)', $unescape)
Assert-AgentTask (Read-Task $serviceXml) 'bridge' 'C:\bvagent.ps1' 'bvagent-install-service.c'
$productXml = Get-GuestAgentTaskXml 'bridge' 'C:\bvagent.ps1'
if ($serviceXml -cne $productXml) {
    $a = $serviceXml -split "`r`n"
    $b = $productXml -split "`r`n"
    $i = 0
    while ($i -lt $a.Count -and $i -lt $b.Count -and $a[$i] -ceq $b[$i]) { $i++ }
    throw "bvagent-install-service.c and Get-GuestAgentTaskXml differ at line $($i + 1): '$($a[$i])' vs '$($b[$i])'"
}
$script:checks++
foreach ($needle in @('TASK_XML_PATH[] = L"C:\\BridgeVM\\bvagent-task.xml"', '/TN \"BridgeVM Guest Agent\" /XML C:\\BridgeVM\\bvagent-task.xml', 'write_task_xml() ? run_and_wait(create_task)')) {
    if (-not $service.Contains($needle)) { throw "bvagent-install-service.c does not register the XML it writes: missing $needle" }
    $script:checks++
}

if ([Environment]::OSVersion.Platform -eq [PlatformID]::Win32NT) {
    $name = 'BridgeVM Agent Task Contract ' + [guid]::NewGuid().ToString('N')
    $agent = 'C:\BridgeVM Contract\bvagent.ps1'
    $script:started = @()
    function Start-ScheduledTask([string]$TaskName) { $script:started += $TaskName }
    try {
        foreach ($round in 1, 2) {
            $identity = Start-VerifiedGuestAgentTask $name $agent
            Assert-AgentTask (Read-Task (Export-ScheduledTask -TaskName $name)) $identity $agent "registered task, round $round"
        }
        Assert-Equal ($script:started -join '|') "$name|$name" 'start requests'
    } finally {
        Unregister-ScheduledTask -TaskName $name -Confirm:$false -ErrorAction SilentlyContinue
    }
}

Write-Output "agent task contract: PASS ($script:checks checks)"
