param([string]$Source = (Join-Path $PSScriptRoot 'win-assets/bvagent-firstboot.ps1'))
$ErrorActionPreference = 'Stop'
# Runs the committed first-logon provisioner unchanged on macOS or Linux, with C: mapped to a temp
# folder and stand-ins for Get-Acl and Set-Acl (Windows-only). C:\BridgeVM must be locked, and
# everything below it reset to inherit the lock, before the receipt is read or any agent file is
# hashed. The scan for reparse points comes after the root is locked (nothing can be planted then);
# a reparse point, or a lock that fails or does not read back, must block provisioning.
# tests/integration/agent-folder-acl-contract.ps1 checks the real permissions on Windows.
if ($IsWindows -or $env:OS -eq 'Windows_NT') { throw 'run this on macOS or Linux; it maps its own C: drive' }
$drive = Join-Path ([IO.Path]::GetTempPath()) ('bvagent-acl-' + [guid]::NewGuid().ToString('N'))
$agent = Join-Path $drive 'BridgeVM/provisioning/agent'
$assets = 'bvagent.ps1', 'bvagent-input.ps1', 'bvagent-unicode-input.cs', 'bvagent-key-input.cs', 'bvagent-pointer-input.cs', 'bvagent-window-inventory.ps1', 'bv-window-inventory.cs', 'bvagent-task.ps1'
$stub = "function Start-VerifiedGuestAgentTask([string]`$TaskName, [string]`$AgentPath) { Set-Content -LiteralPath 'C:\BridgeVM\provisioning\task-started.txt' -Value `$TaskName; 'BRIDGEVM\stand-in' }`n"
$locked = 'O:S-1-5-32-544D:PAI(A;OICI;FA;;;S-1-5-18)(A;OICI;FA;;;S-1-5-32-544)(A;OICI;0x1200a9;;;S-1-5-32-545)'

# Windows, reduced to what the provisioner relies on: an object's owner and DACL are what was set
# last (in SDDL), and its entries are its own plus, unless protected, its folder's inheritable ones.
# C:\BridgeVM starts with what it inherits from C:\: Users read, Authenticated Users modify.
$global:BvAcl = @{ State = @{}; Events = [Collections.Generic.List[string]]::new(); Fail = $null; Ignore = $null
    Rights = @{ 'FA' = 'FullControl'; '0x1200a9' = 'ReadAndExecute, Synchronize'; '0x1301bf' = 'Modify, Synchronize' } }
function global:Get-BvName([string]$Path) {
    $full = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($Path)
    return $full.Substring($drive.Length + 1).Replace('\', '/')
}
function global:Get-BvEntries([string]$Name) {
    $sddl = $global:BvAcl.State[$Name]
    if ($null -eq $sddl) { $sddl = 'O:S-1-5-32-544D:AI' }
    foreach ($m in [regex]::Matches($sddl, '\(A;([A-Z]*);([^;]+);;;([^)]+)\)')) {
        [pscustomobject]@{ Flags = $m.Groups[1].Value; Rights = $m.Groups[2].Value; Sid = $m.Groups[3].Value; Inherited = $m.Groups[1].Value.Contains('ID') }
    }
    $cut = $Name.LastIndexOf('/')
    if ($sddl -notmatch 'D:P' -and $cut -gt 0) {
        foreach ($entry in @(Get-BvEntries $Name.Substring(0, $cut))) {
            if ($entry.Flags.Contains('OI')) { [pscustomobject]@{ Flags = $entry.Flags; Rights = $entry.Rights; Sid = $entry.Sid; Inherited = $true } }
        }
    }
}
function global:Get-FileHash([string]$LiteralPath, [string]$Algorithm) {
    $global:BvAcl.Events.Add('hash ' + (Get-BvName $LiteralPath))
    Microsoft.PowerShell.Utility\Get-FileHash -LiteralPath $LiteralPath -Algorithm $Algorithm
}
function global:Get-Content {
    param([string]$LiteralPath, [switch]$Raw)
    $global:BvAcl.Events.Add('read ' + (Get-BvName $LiteralPath))
    Microsoft.PowerShell.Management\Get-Content -LiteralPath $LiteralPath -Raw:$Raw
}
function global:Get-Item([string]$LiteralPath, [switch]$Force) {
    $global:BvAcl.Events.Add('item ' + (Get-BvName $LiteralPath))
    Microsoft.PowerShell.Management\Get-Item -LiteralPath $LiteralPath -Force:$Force
}
function global:Get-ChildItem([string]$LiteralPath, [switch]$Recurse, [switch]$Force) {
    $global:BvAcl.Events.Add('list ' + (Get-BvName $LiteralPath))
    Microsoft.PowerShell.Management\Get-ChildItem -LiteralPath $LiteralPath -Recurse:$Recurse -Force:$Force
}
function global:Set-Acl([string]$LiteralPath, $AclObject) {
    $name = Get-BvName $LiteralPath
    $global:BvAcl.Events.Add('set ' + $name)
    if ($name -eq $global:BvAcl.Fail) { throw 'Attempted to perform an unauthorized operation.' }
    if ($name -ne $global:BvAcl.Ignore) { $global:BvAcl.State[$name] = $AclObject.Sddl }
}
function global:Get-Acl([string]$LiteralPath) {
    $acl = [pscustomobject]@{ Name = (Get-BvName $LiteralPath); Sddl = $null }
    $acl.Sddl = $global:BvAcl.State[$acl.Name]
    if ($null -eq $acl.Sddl) { $acl.Sddl = 'O:S-1-5-32-544D:AI' }
    $acl | Add-Member ScriptMethod SetSecurityDescriptorSddlForm { param($Sddl, $Sections) $this.Sddl = $Sddl }
    $acl | Add-Member ScriptMethod GetSecurityDescriptorSddlForm { param($Sections) $this.Sddl }
    $acl | Add-Member ScriptMethod GetOwner { param($Type) [pscustomobject]@{ Value = ($this.Sddl -replace '^O:(.*?)D:.*$', '$1') } }
    $acl | Add-Member ScriptProperty AreAccessRulesProtected { $this.Sddl -match 'D:P' }
    $acl | Add-Member ScriptMethod GetAccessRules {
        param($Explicit, $Inherited, $Type)
        foreach ($entry in @(Get-BvEntries $this.Name)) {
            [pscustomobject]@{ IdentityReference = $entry.Sid; AccessControlType = 'Allow'; FileSystemRights = $global:BvAcl.Rights[$entry.Rights]; IsInherited = $entry.Inherited }
        }
    }
    return $acl
}

function Invoke-Case([string]$Fail, [string]$Ignore, [string]$Link) {
    foreach ($dir in 'BridgeVM', 'elsewhere') {
        if (Test-Path -LiteralPath (Join-Path $drive $dir)) { Remove-Item -LiteralPath (Join-Path $drive $dir) -Recurse -Force }
    }
    $global:BvAcl.State.Clear(); $global:BvAcl.Events.Clear(); $global:BvAcl.Fail = $Fail; $global:BvAcl.Ignore = $Ignore
    $global:BvAcl.State['BridgeVM'] = 'O:S-1-5-32-544D:AI(A;OICIID;FA;;;S-1-5-18)(A;OICIID;FA;;;S-1-5-32-544)(A;OICIID;0x1200a9;;;S-1-5-32-545)(A;OICIID;0x1301bf;;;S-1-5-11)'
    # As WinPE's xcopy leaves it, plus one file someone granted Everyone (S-1-1-0) write to.
    [void](New-Item -ItemType Directory -Path $agent)
    [void](New-Item -ItemType Directory -Path (Join-Path $drive 'BridgeVM/provisioning/drivers'))
    [IO.File]::WriteAllText((Join-Path $drive 'BridgeVM/provisioning/drivers/vioser.inf'), 'synthetic driver')
    $receipt = @("schema`tbridgevm-windows-guest-payload-receipt-v1", "architecture`tarm64")
    foreach ($name in $assets + 'bvagent-firstboot.ps1') {
        $path = Join-Path $agent $name
        if ($name -eq 'bvagent-firstboot.ps1') { Copy-Item -LiteralPath $Source -Destination $path }
        elseif ($name -eq 'bvagent-task.ps1') { [IO.File]::WriteAllText($path, $stub) }
        else { [IO.File]::WriteAllText($path, 'synthetic asset ' + $name) }
        if ($name -ne 'bvagent-firstboot.ps1') {
            $receipt += "guest_tool`tagent/$name`t" + (Microsoft.PowerShell.Utility\Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant()
        }
    }
    $global:BvAcl.State['BridgeVM/provisioning/agent/bvagent-input.ps1'] = 'O:S-1-5-21-1-1001D:AI(A;;0x1301bf;;;S-1-1-0)'
    [IO.File]::WriteAllText((Join-Path $drive 'BridgeVM/provisioning/payload-receipt.tsv'), (($receipt -join "`n") + "`n"))
    if ($Link) {  # move the folder away and leave a link to it in its place
        $target = Join-Path $drive 'elsewhere'
        Move-Item -LiteralPath (Join-Path $drive $Link) -Destination $target
        [void](New-Item -ItemType SymbolicLink -Path (Join-Path $drive $Link) -Target $target)
    }
    $staged = @(Microsoft.PowerShell.Management\Get-ChildItem -LiteralPath (Join-Path $drive 'BridgeVM') -Recurse -Force | ForEach-Object { 'set ' + (Get-BvName $_.FullName) })
    $global:LASTEXITCODE = $null
    & 'C:\BridgeVM\provisioning\agent\bvagent-firstboot.ps1' | Out-Null
    $log = Join-Path $drive 'BridgeVM/guest-tools-firstboot.log'
    return @{
        Exit = $LASTEXITCODE; Log = $(if (Test-Path -LiteralPath $log) { [IO.File]::ReadAllText($log) } else { '' })
        Started = (Test-Path -LiteralPath (Join-Path $drive 'BridgeVM/provisioning/task-started.txt'))
        Events = @($global:BvAcl.Events); Staged = @(@('set BridgeVM', 'set BridgeVM/guest-tools-firstboot.log') + $staged)
    }
}

[void](New-Item -ItemType Directory -Path $drive)
try {
    New-PSDrive -Name C -PSProvider FileSystem -Root $drive -Scope Global | Out-Null
    $result = Invoke-Case
    if ($result.Exit -ne 0 -or $result.Log -notmatch 'BVAGENT PROVISION STAGED' -or -not $result.Started) {
        throw ('valid payload was not provisioned (exit ' + $result.Exit + '): ' + $result.Log.Trim())
    }
    $sets = @($result.Events | Where-Object { $_.StartsWith('set ') })
    $firstUse = [array]::FindIndex([string[]]$result.Events, [Predicate[string]] { param($e) $e -match '^(hash|read) BridgeVM/provisioning/' })
    if ($firstUse -lt 0 -or $sets[0] -ne 'set BridgeVM' -or (Compare-Object $sets $result.Staged) -or ($result.Events.IndexOf($sets[-1]) -gt $firstUse)) {
        throw ('C:\BridgeVM and everything below it were not locked before the payload was read: ' + ($result.Events -join ', '))
    }
    # The order: is the root a link, lock it, then scan below it, then reset what the scan found.
    $order = @($result.Events | Where-Object { $_ -match '^(item|list) ' -or $_ -eq 'set BridgeVM' }) + $sets[1]
    if (($order -join ', ') -cne ('item BridgeVM, set BridgeVM, list BridgeVM, ' + $sets[1])) {
        throw ('C:\BridgeVM was not locked before the scan for reparse points: ' + ($result.Events -join ', '))
    }
    if ($global:BvAcl.State['BridgeVM'] -cne $locked) { throw ('C:\BridgeVM was locked as ' + $global:BvAcl.State['BridgeVM']) }
    $blocked = 'BVAGENT PROVISION BLOCKED: C:\BridgeVM could not be locked so that only SYSTEM and Administrators can change it: '
    $cases = @(
        @{ Name = 'failed lock'; Fail = 'BridgeVM'; Expect = $blocked + 'Attempted to perform an unauthorized operation.' },
        @{ Name = 'lock that does not read back'; Ignore = 'BridgeVM'; Expect = $blocked + 'the folder reads back as ' },
        @{ Name = 'file that keeps its own entry'; Ignore = 'BridgeVM/provisioning/agent/bvagent-input.ps1'; Expect = 'bvagent-input.ps1 does not just inherit the folder' },
        @{ Name = 'C:\BridgeVM that is a link'; Link = 'BridgeVM'; Sets = @(); Expect = 'BridgeVM is a reparse point (a junction or link)' },
        @{ Name = 'agent folder that is a link'; Link = 'BridgeVM/provisioning/agent'; Sets = @('set BridgeVM'); Expect = 'agent is a reparse point (a junction or link)' },
        @{ Name = 'link below C:\BridgeVM'; Link = 'BridgeVM/provisioning/drivers'; Sets = @('set BridgeVM'); Expect = 'drivers is a reparse point (a junction or link)' }
    )
    foreach ($case in $cases) {
        $result = Invoke-Case $case.Fail $case.Ignore $case.Link
        # A link below the root is refused with the root locked and nothing below it reset; a root
        # that is itself a link is refused before anything is set.
        $early = $case.Link -and ((@($result.Events -match '^set ') -join ', ') -cne ($case.Sets -join ', ') -or
            ($case.Sets.Count -and $global:BvAcl.State['BridgeVM'] -cne $locked))
        if ($result.Exit -ne 1 -or -not $result.Log.Contains($blocked) -or -not $result.Log.Contains($case.Expect) -or $result.Started -or $early -or ($result.Events -match '^(hash|read) BridgeVM/provisioning/')) {
            throw ($case.Name + ' did not block provisioning before the payload was read (exit ' + $result.Exit + '): ' + $result.Log.Trim() + ' events: ' + ($result.Events -join ', '))
        }
    }
    Write-Output 'Firstboot C:\BridgeVM lock: PASS (stand-ins for Get-Acl and Set-Acl)'
} finally {
    Remove-PSDrive -Name C -Scope Global -ErrorAction SilentlyContinue
    if (Test-Path -LiteralPath $drive) { Remove-Item -LiteralPath $drive -Recurse -Force }
    foreach ($name in 'Get-Acl', 'Set-Acl', 'Get-FileHash', 'Get-Content', 'Get-BvName', 'Get-BvEntries') { Remove-Item -LiteralPath ('function:global:' + $name) -ErrorAction SilentlyContinue }
}
