# The guest agent's printer to the Mac (PRINTWATCH, bvagent.ps1 "print"):
# 1. Setup with mocked printer cmdlets: 'Mac (BridgeVM)' on the inbox Microsoft Print To PDF driver and a
#    Local Port named with a file path in the user's folder, each made only when missing, a printer on
#    another port or driver made again, a failure reported as one BVPRINT printer=failed line with the
#    watch off, and PRINTWATCH off removing the printer and its port.
# 2. Job detection by the watch thread as compiled in bvagent.ps1 (BvClipWatch ClaimPrint/TakePrintJobs)
#    on a temporary folder: a finished job.pdf is renamed and announced once with its size, a job still
#    being written is never announced, nothing is read through a reparse point, at most MaxPrintJobs
#    wait, and a restarted watch announces the jobs still there.
# 3. Read-Line writes PRINTJOB between replies only while printing is watched.
# No spooler and no printer: the printer cmdlets are functions here.
param([ValidateSet('all', 'readline')][string]$Mode = 'all')
$ErrorActionPreference = 'Stop'
$agentPath = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '../../scripts/win-assets/bvagent.ps1')).Path
$tokens = $null; $errors = $null
$ast = [System.Management.Automation.Language.Parser]::ParseFile($agentPath, [ref]$tokens, [ref]$errors)
if ($errors.Count -ne 0) { throw 'agent parse failed' }
function Get-AgentFunction([string]$Name) {
    $node = $ast.Find({ param($n) $n -is [System.Management.Automation.Language.FunctionDefinitionAst] -and $n.Name -eq $Name }, $true)
    if ($null -eq $node) { throw "missing agent function $Name" }
    return $node.Extent.Text
}
function Assert($Condition, [string]$What) { if (-not $Condition) { throw "FAIL: $What" } }
$onWindows = [Environment]::OSVersion.Platform -eq 'Win32NT'

if ($Mode -eq 'readline') {
    # Read-Line with scripted reads: -3 is the watch thread's wake for finished print jobs.
    Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Text;
namespace BridgeVM {
    public static class BvClipWatch {
        public static Queue<int> Reads = new Queue<int>();
        public static int ReadCalls;
        public static string[] Jobs = new string[0];
        public static int Read(IntPtr h, byte[] buf) {
            ReadCalls++;
            int r = Reads.Count > 0 ? Reads.Dequeue() : 0;
            if (r <= 0) { return r; }
            byte[] line = Encoding.ASCII.GetBytes("PING\n");
            Array.Copy(line, buf, line.Length);
            return line.Length;
        }
        public static string[] TakeLinks() { return new string[0]; }
        public static string[] TakePrintJobs() { string[] jobs = Jobs; Jobs = new string[0]; return jobs; }
    }
    public static class FakeKernel {
        public static int ReadFileCalls;
        public static bool ReadFile(IntPtr h, byte[] buf, uint toRead, out uint read, IntPtr overlapped) {
            ReadFileCalls++;
            byte[] line = Encoding.ASCII.GetBytes("PING\n");
            Array.Copy(line, buf, line.Length);
            read = (uint)line.Length;
            return true;
        }
    }
}
'@
    $K = [BridgeVM.FakeKernel]
    function Is-BadHandle($handle) { return $false }
    function Write-Line($handle, [string]$line, [string]$what) { [void]$script:written.Add($line) }
    . ([scriptblock]::Create((Get-AgentFunction 'Read-Line')))
    $job1 = [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes('C:\Users\u\AppData\Local\BridgeVM\Print\0638982000000000001.pdf')) + ' 1234'
    $job2 = [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes('C:\Users\u\AppData\Local\BridgeVM\Print\0638982000000000002.pdf')) + ' 5'
    foreach ($case in @(@('print', $true, @(-3, 5), "PRINTJOB $job1|PRINTJOB $job2"), @('off', $false, @(), ''))) {
        $script:BvClipPush = $false; $script:BvLinkWatch = $false; $script:BvPrintWatch = $case[1]
        $script:BvReadBuffer = New-Object byte[] 16384
        $script:BvPending = New-Object System.Text.StringBuilder
        $script:BvLines = New-Object 'System.Collections.Generic.Queue[string]'
        $script:written = New-Object System.Collections.ArrayList
        [BridgeVM.BvClipWatch]::Reads.Clear(); foreach ($r in $case[2]) { [BridgeVM.BvClipWatch]::Reads.Enqueue($r) }
        [BridgeVM.BvClipWatch]::Jobs = @($job1, $job2)
        [BridgeVM.BvClipWatch]::ReadCalls = 0; [BridgeVM.FakeKernel]::ReadFileCalls = 0
        $line = Read-Line ([IntPtr]5)
        $wrote = $script:written -join '|'
        Assert ($line -ceq 'PING' -and $wrote -ceq $case[3]) "Read-Line $($case[0]): returned '$line', wrote '$wrote'"
        $threadReader = [BridgeVM.BvClipWatch]::ReadCalls -gt 0
        Assert ($threadReader -eq $case[1] -and ([BridgeVM.FakeKernel]::ReadFileCalls -gt 0) -ne $case[1]) "Read-Line $($case[0]): wrong reader"
    }
    Write-Output 'PASS: Read-Line writes each PRINTJOB between replies while printing is watched, and reads as before otherwise'
    return
}

$assign = $ast.Find({ param($n) $n -is [System.Management.Automation.Language.AssignmentStatementAst] -and $n.Left.Extent.Text -eq '$script:BvClipWatchSource' }, $true)
$watchSource = $assign.Right.Expression.Value
if ($PSVersionTable.PSEdition -eq 'Core') {
    # Windows PowerShell 5.1 compiles C# 5; prove the watch thread does too.
    Add-Type -TypeDefinition $watchSource -CompilerOptions '/langversion:5' -ErrorAction Stop
} else {
    Add-Type -TypeDefinition $watchSource -ErrorAction Stop
}
$printBlock = [regex]::Match($ast.Extent.Text, '(?s)# >>> print\r?\n.*?# <<< print')
Assert $printBlock.Success 'print block'
function Log([string]$m) { }
. ([scriptblock]::Create($printBlock.Value))
Assert ($script:BvPrinterName -ceq 'Mac (BridgeVM)' -and $script:BvPrintDriver -ceq 'Microsoft Print To PDF') 'printer name and inbox driver'
Assert ((Get-BvPrintPort).EndsWith([IO.Path]::Combine('BridgeVM', 'Print', 'job.pdf'))) 'the port is job.pdf in the user''s BridgeVM\Print'
Assert ($ast.Extent.Text.Contains("'PRINTWATCH' { Write-CommandResult `$h (Start-PrintWatch `$arg) }")) 'PRINTWATCH dispatch'

$work = Join-Path ([IO.Path]::GetTempPath()) ('bv-print-' + [guid]::NewGuid().ToString('N'))
$dir = Join-Path $work 'BridgeVM/Print'
$port = Join-Path $dir 'job.pdf'
function Get-BvPrintPort { return $port }
try {
    # 1. Setup, against a model of the spooler's drivers, ports and printers.
    $script:model = $null
    $script:calls = New-Object System.Collections.ArrayList
    $script:fail = @{}
    function Reset-Model([string[]]$Drivers = @('Microsoft Print To PDF'), [string[]]$Ports = @(), [hashtable]$Printers = @{}) {
        $script:model = @{ Drivers = [System.Collections.ArrayList]@($Drivers); Ports = [System.Collections.ArrayList]@($Ports); Printers = $Printers }
        $script:calls.Clear(); $script:fail = @{}
    }
    function Invoke-Mock([string]$What) {
        [void]$script:calls.Add($What)
        if ($script:fail.ContainsKey($What.Split(' ')[0])) { throw $script:fail[$What.Split(' ')[0]] }
    }
    function Get-PrinterDriver { [CmdletBinding()] param([string]$Name) if ($script:model.Drivers -contains $Name) { [pscustomobject]@{ Name = $Name } } }
    function Add-PrinterDriver { [CmdletBinding()] param([string]$Name) Invoke-Mock "Add-PrinterDriver $Name"; [void]$script:model.Drivers.Add($Name) }
    function Get-PrinterPort { [CmdletBinding()] param([string]$Name) if ($script:model.Ports -contains $Name) { [pscustomobject]@{ Name = $Name } } }
    function Add-PrinterPort { [CmdletBinding()] param([string]$Name) Invoke-Mock "Add-PrinterPort $Name"; [void]$script:model.Ports.Add($Name) }
    function Remove-PrinterPort { [CmdletBinding()] param([string]$Name) Invoke-Mock "Remove-PrinterPort $Name"; $script:model.Ports.Remove($Name) }
    function Get-Printer { [CmdletBinding()] param([string]$Name) if ($script:model.Printers.ContainsKey($Name)) { $script:model.Printers[$Name] } }
    function Add-Printer { [CmdletBinding()] param([string]$Name, [string]$DriverName, [string]$PortName)
        Invoke-Mock "Add-Printer $Name|$DriverName|$PortName"
        if ($script:model.Drivers -notcontains $DriverName -or $script:model.Ports -notcontains $PortName) { throw 'The specified driver or port does not exist.' }
        $script:model.Printers[$Name] = [pscustomobject]@{ Name = $Name; DriverName = $DriverName; PortName = $PortName }
    }
    function Remove-Printer { [CmdletBinding()] param([string]$Name) Invoke-Mock "Remove-Printer $Name"; $script:model.Printers.Remove($Name) }
    function Expect-Watch([string]$Arg, [int]$Exit, [string]$Out, [string[]]$Calls, [bool]$Watching, [string]$What) {
        $script:calls.Clear()
        $result = Start-PrintWatch $Arg
        $made = @($script:calls) -join ';'
        Assert ($result.Exit -eq $Exit -and $result.Out -ceq $Out) "$What`: Exit=$($result.Exit) Out='$($result.Out)'"
        Assert ($made -ceq ($Calls -join ';')) "$What`: calls '$made'"
        Assert ($script:BvPrintWatch -eq $Watching) "$What`: watching=$($script:BvPrintWatch)"
    }
    $add = "Add-Printer Mac (BridgeVM)|Microsoft Print To PDF|$port"
    $printer = { param($Driver, $Port) @{ 'Mac (BridgeVM)' = [pscustomobject]@{ Name = 'Mac (BridgeVM)'; DriverName = $Driver; PortName = $Port } } }

    Reset-Model
    Expect-Watch '' 0 'BVPRINT printer=ready' @("Add-PrinterPort $port", $add) $true 'first boot'
    Assert ([IO.Directory]::Exists($dir)) 'the print folder is made'
    Expect-Watch '' 0 'BVPRINT printer=ready' @() $true 'a later boot changes nothing'
    Reset-Model -Ports @($port, 'C:\old\job.pdf') -Printers (& $printer 'Microsoft Print To PDF' 'C:\old\job.pdf')
    Expect-Watch '' 0 'BVPRINT printer=ready' @('Remove-Printer Mac (BridgeVM)', $add) $true 'a printer on another port is made again'
    Reset-Model -Ports @($port) -Printers (& $printer 'Generic / Text Only' $port)
    Expect-Watch '' 0 'BVPRINT printer=ready' @('Remove-Printer Mac (BridgeVM)', $add) $true 'a printer with another driver is made again'
    Reset-Model -Drivers @()
    Expect-Watch '' 0 'BVPRINT printer=ready' @('Add-PrinterDriver Microsoft Print To PDF', "Add-PrinterPort $port", $add) $true 'the inbox driver is added from the driver store'

    # A failure is one line and the watch stays off; the next PRINTWATCH tries again.
    Expect-Watch 'off' 0 'BVPRINT printer=removed' @('Remove-Printer Mac (BridgeVM)', "Remove-PrinterPort $port") $false 'off removes the printer and its port'
    Reset-Model -Drivers @()
    $script:fail['Add-PrinterDriver'] = 'The specified driver does not exist in the driver store.'
    Expect-Watch '' 1 'BVPRINT printer=failed detail=The specified driver does not exist in the driver store.' @('Add-PrinterDriver Microsoft Print To PDF') $false 'no Print To PDF driver'
    Reset-Model
    $script:fail['Add-Printer'] = "Access was denied`r`n to the specified resource."
    Expect-Watch '' 1 'BVPRINT printer=failed detail=Access was denied to the specified resource.' @("Add-PrinterPort $port", $add) $false 'Add-Printer refused, on one line'
    Reset-Model
    $script:fail['Add-PrinterPort'] = 'The spooler service is not reachable.'
    Expect-Watch '' 1 'BVPRINT printer=failed detail=The spooler service is not reachable.' @("Add-PrinterPort $port") $false 'no spooler'
    Reset-Model
    Expect-Watch 'off' 0 'BVPRINT printer=absent' @() $false 'off with nothing there'
    Reset-Model -Ports @($port) -Printers (& $printer 'Microsoft Print To PDF' $port)
    $script:fail['Remove-PrinterPort'] = 'The port is in use.'
    Expect-Watch 'off' 1 'BVPRINT printer=remove-failed detail=The port is in use.' @('Remove-Printer Mac (BridgeVM)', "Remove-PrinterPort $port") $false 'off, port still in use'
    Write-Output 'PASS: PRINTWATCH makes only what is missing, makes a wrong printer again, reports a failure as one line with the watch off, and off removes printer and port'

    # 2. Job detection: the watch thread renames, TakePrintJobs announces.
    $watch = [BridgeVM.BvClipWatch]
    function Take { @($watch::TakePrintJobs()) }
    function Wait-Jobs([int]$Count, [string]$What) {
        $got = @()
        $end = [DateTime]::UtcNow.AddSeconds(5)
        while ($got.Count -lt $Count -and [DateTime]::UtcNow -lt $end) { Start-Sleep -Milliseconds 50; $got += @(Take) }
        Assert ($got.Count -eq $Count) "$What`: $($got.Count) jobs, wanted $Count"
        return , $got
    }
    function Get-Job([string]$Line) {
        $fields = $Line.Split(' ')
        Assert ($fields.Count -eq 2) "job line '$Line'"
        return @{ Path = [Text.Encoding]::UTF8.GetString([Convert]::FromBase64String($fields[0])); Bytes = [long]$fields[1] }
    }
    function Write-Job([string]$Path, [byte[]]$Bytes) { [IO.File]::WriteAllBytes($Path, $Bytes) }
    function Get-Pdf([int]$Length) { $b = New-Object byte[] $Length; [Text.Encoding]::ASCII.GetBytes('%PDF-1.7').CopyTo($b, 0); return , $b }
    $watch::StopPrint()
    Reset-Model
    Expect-Watch '' 0 'BVPRINT printer=ready' @("Add-PrinterPort $port", $add) $true 'watch on'
    [void](Take)
    Write-Job $port (Get-Pdf 3000)
    $first = Get-Job (Wait-Jobs 1 'a finished job')[0]
    Assert ([regex]::IsMatch([IO.Path]::GetFileName($first.Path), '\A[0-9]{19}\.pdf\z') -and [IO.Path]::GetDirectoryName($first.Path) -ceq $dir) "renamed beside the port: $($first.Path)"
    Assert ($first.Bytes -eq 3000 -and ([IO.File]::ReadAllBytes($first.Path)).Length -eq 3000 -and -not [IO.File]::Exists($port)) 'its size, and job.pdf is free for the next job'
    Start-Sleep -Milliseconds 300
    Assert ((Take).Count -eq 0) 'announced once'

    # The spooler holds the file open while it writes (share mode Read): never announced half-written.
    $stream = New-Object IO.FileStream($port, [IO.FileMode]::Create, [IO.FileAccess]::Write, [IO.FileShare]::Read)
    $half = Get-Pdf 5000
    $stream.Write($half, 0, 2500); $stream.Flush()
    $end = [DateTime]::UtcNow.AddSeconds(1.5)
    while ([DateTime]::UtcNow -lt $end) { Start-Sleep -Milliseconds 50; Assert ((Take).Count -eq 0) 'a job being written is never announced' }
    $stream.Write($half, 2500, 2500); $stream.Dispose()
    $second = Get-Job (Wait-Jobs 1 'the job once written')[0]
    Assert ($second.Bytes -eq 5000 -and $second.Path -cne $first.Path) "the whole job: $($second.Bytes) bytes"

    # A restarted watch (a new agent) announces the jobs still there, oldest first.
    $watch::StartPrint($port)
    $jobs = Wait-Jobs 2 'after a restart'
    $again = @($jobs | ForEach-Object { (Get-Job $_).Path })
    Assert (($again -join '|') -ceq ((@($first.Path, $second.Path) | Sort-Object) -join '|')) "again: $($again -join '|')"

    # Never through a reparse point: a link in place of a job, of the port file or of the folder.
    $secret = Join-Path $work 'secret.pdf'
    Write-Job $secret (Get-Pdf 77)
    [void](New-Item -ItemType SymbolicLink -Path (Join-Path $dir '0000000000000000001.pdf') -Value $secret)
    [void](New-Item -ItemType SymbolicLink -Path $port -Value $secret)
    Start-Sleep -Milliseconds 600
    Assert ((Take).Count -eq 0) 'a link is never announced'
    Assert (([IO.File]::GetAttributes($port) -band [IO.FileAttributes]::ReparsePoint) -ne 0) 'a link as job.pdf is never renamed'
    [IO.File]::Delete($port); [IO.File]::Delete((Join-Path $dir '0000000000000000001.pdf'))
    $other = Join-Path $work 'other'
    [void][IO.Directory]::CreateDirectory($other)
    Write-Job (Join-Path $other '0000000000000000002.pdf') (Get-Pdf 10)
    Write-Job (Join-Path $other 'job.pdf') (Get-Pdf 11)
    [IO.Directory]::Move($dir, $dir + '.real')
    if ($onWindows) { [void](New-Item -ItemType Junction -Path $dir -Value $other) } else { [void](New-Item -ItemType SymbolicLink -Path $dir -Value $other) }
    $watch::StartPrint($port)
    Start-Sleep -Milliseconds 600
    Assert ((Take).Count -eq 0 -and [IO.File]::Exists((Join-Path $other 'job.pdf'))) 'a linked folder is never read or renamed in'
    [IO.Directory]::Delete($dir)
    [IO.Directory]::Move($dir + '.real', $dir)
    $watch::StartPrint($port)
    [void](Wait-Jobs 2 'the real folder again')

    # At most MaxPrintJobs wait beside the port; the next one stays job.pdf until the host takes some.
    $max = $watch::MaxPrintJobs
    for ($i = 2; $i -lt $max; $i++) { Write-Job (Join-Path $dir ('{0:D19}.pdf' -f (100 + $i))) (Get-Pdf 9) }
    Assert ((Wait-Jobs ($max - 2) 'filling up').Count -eq $max - 2) 'filled'
    Write-Job $port (Get-Pdf 12)
    Start-Sleep -Milliseconds 600
    Assert ([IO.File]::Exists($port) -and (Take).Count -eq 0) 'past MaxPrintJobs job.pdf waits'
    [IO.File]::Delete($first.Path)
    $last = Get-Job (Wait-Jobs 1 'room again')[0]
    Assert ($last.Bytes -eq 12) 'taken once there is room'

    # Stopped: nothing is renamed or announced.
    $watch::StopPrint()
    Write-Job $port (Get-Pdf 13)
    Start-Sleep -Milliseconds 600
    Assert ([IO.File]::Exists($port) -and (Take).Count -eq 0) 'stopped'
    Write-Output 'PASS: a finished job is renamed and announced once with its size, never while written, never through a reparse point, at most MaxPrintJobs wait, a restarted watch announces the rest, a stopped one nothing'
} finally {
    [BridgeVM.BvClipWatch]::StopPrint()
    Remove-Item -LiteralPath $work -Recurse -Force -ErrorAction SilentlyContinue
}

& (Get-Process -Id $PID).Path -NoProfile -File $PSCommandPath -Mode readline
if ($LASTEXITCODE -ne 0) { throw 'Read-Line dispatch failed' }
Write-Output 'PASS: the agent printer to the Mac (setup, job detection, PRINTJOB dispatch)'
