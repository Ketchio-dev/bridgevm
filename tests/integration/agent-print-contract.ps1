# The guest agent's printer to the Mac (PRINTWATCH, bvagent.ps1 "print"):
# 1. Setup with mocked printer cmdlets: 'Mac (BridgeVM)' on the inbox Microsoft Print To PDF driver and a
#    Local Port named with a file path in the user's folder, each made only when missing, a printer on
#    another port or driver made again, a failure reported as one BVPRINT printer=failed line with the
#    watch off, a printer that cannot be paused removed, and PRINTWATCH off removing the printer and port.
# 2. Job detection by the watch thread as compiled in bvagent.ps1 (BvClipWatch ClaimPrint/TakePrintJobs)
#    on a temporary folder: a finished job.pdf is renamed and announced once with its size, a job still
#    being written is never announced, nothing is read through a reparse point, and a restarted watch
#    announces the jobs still there.
# 3. The print gate (BvClipWatch.PrintGate) against a model of the spooler (SpoolerModel below: a queue,
#    the printer's pause, one port file that each job starting truncates): the printer is paused at
#    watch start and at agent start; jobs go out one at a time, none while job.pdf holds one not renamed,
#    none at MaxPrintJobs renamed; bursts and jobs arriving while one is out all arrive, none lost.
# 4. Read-Line writes PRINTJOB between replies only while printing is watched.
# No spooler and no printer: the printer cmdlets are functions here, the queue a model.
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
# The watch thread as bvagent.ps1 compiles it, with a model of the spooler in the same assembly: no
# printer, no winspool. The model's own thread is the spooler: while the printer runs, the first job not
# paused starts, opens the port file CREATE_ALWAYS as localmon does (a job.pdf still there is a job
# lost), writes half its document, waits PrintMs, writes the rest, closes the file and leaves the queue.
# On Windows the spooler's open handle (share mode Read) makes the watch's rename fail until the job
# closes the file; macOS renames an open file. So the model writes beside the port and puts the whole
# file in place as it closes it: on every system the watch can claim job.pdf only once the job is done,
# possibly after the job has left the queue, as on Windows.
$modelSource = @'
namespace BridgeVM {
    public sealed class SpoolerModel : IBvPrintQueue {
        public static SpoolerModel Current;
        public static int Opened;
        public static void Install(SpoolerModel model) {
            Current = model;
            BvClipWatch.OpenPrintQueue = delegate (string printer) { Opened++; return Current; };
        }
        readonly object gate = new object();
        readonly System.Collections.Generic.List<int[]> queue = new System.Collections.Generic.List<int[]>();
        readonly System.Collections.Generic.Dictionary<int, byte[]> docs = new System.Collections.Generic.Dictionary<int, byte[]>();
        public readonly System.Collections.Generic.List<int> Started = new System.Collections.Generic.List<int>();
        public volatile bool PrinterPaused;
        public volatile bool Readable = true;
        public volatile bool Pausable = true;
        public volatile string Port;
        public volatile int PrintMs = 30;
        public int Lost, Printed, Closes;
        int nextId = 1;
        volatile bool stopped;

        public int Add(byte[] doc) {
            lock (gate) { int id = nextId++; queue.Add(new int[] { id, 0 }); docs[id] = doc; return id; }
        }
        public int Queued { get { lock (gate) { return queue.Count; } } }
        public int PausedJobs {
            get { lock (gate) { int n = 0; foreach (int[] job in queue) { if ((job[1] & 1) != 0) { n++; } } return n; } }
        }
        public string StartOrder { get { lock (gate) { return string.Join(",", Started.ToArray()); } } }
        public BvPrintJob[] Jobs() {
            lock (gate) {
                if (!Readable) { return null; }
                BvPrintJob[] jobs = new BvPrintJob[queue.Count];
                for (int i = 0; i < queue.Count; i++) { jobs[i] = new BvPrintJob(queue[i][0], queue[i][1]); }
                return jobs;
            }
        }
        public bool? Paused() {
            lock (gate) { if (!Readable) { return null; } return PrinterPaused; }
        }
        public bool PausePrinter(bool pause) {
            lock (gate) {
                if (pause && !Pausable) { return false; }
                PrinterPaused = pause;
                return true;
            }
        }
        public bool PauseJob(int id, bool pause) {
            lock (gate) {
                foreach (int[] job in queue) {
                    if (job[0] == id) { job[1] = pause ? job[1] | 1 : job[1] & ~1; return true; }
                }
                return false;
            }
        }
        public void Close() { lock (gate) { Closes++; } }
        public void Start() {
            System.Threading.Thread thread = new System.Threading.Thread(Run);
            thread.IsBackground = true;
            thread.Start();
        }
        public void Stop() { stopped = true; }

        void Run() {
            while (!stopped) {
                System.Threading.Thread.Sleep(2);
                int id = 0;
                byte[] doc = null;
                string port = Port;
                lock (gate) {
                    if (PrinterPaused || port == null) { continue; }
                    foreach (int[] job in queue) {
                        if ((job[1] & 1) == 0) { id = job[0]; job[1] |= 0x10; doc = docs[id]; break; }
                    }
                    if (id == 0) { continue; }
                    Started.Add(id);
                    if (System.IO.File.Exists(port)) { Lost++; System.IO.File.Delete(port); }
                }
                string spooling = System.IO.Path.Combine(System.IO.Path.GetDirectoryName(port), "spooling.tmp");
                using (System.IO.FileStream stream = new System.IO.FileStream(spooling, System.IO.FileMode.Create, System.IO.FileAccess.Write, System.IO.FileShare.Read)) {
                    stream.Write(doc, 0, doc.Length / 2);
                    stream.Flush();
                    System.Threading.Thread.Sleep(PrintMs);
                    stream.Write(doc, doc.Length / 2, doc.Length - doc.Length / 2);
                }
                lock (gate) {
                    if (System.IO.File.Exists(port)) { Lost++; System.IO.File.Delete(port); }
                    System.IO.File.Move(spooling, port);
                }
                lock (gate) {
                    queue.RemoveAll(delegate (int[] job) { return job[0] == id; });
                    docs.Remove(id);
                    Printed++;
                }
            }
        }
    }
}
'@
$watchSource = $assign.Right.Expression.Value + "`n" + $modelSource
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
$script:printerKnown = $false
function Test-BvPrinterExists { return $script:printerKnown }
$spool = New-Object BridgeVM.SpoolerModel
[BridgeVM.SpoolerModel]::Install($spool)
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
    Assert ($spool.PrinterPaused) 'the new printer is paused before the watch takes it'
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
    # A printer the watch cannot pause would lose jobs: it goes.
    Reset-Model
    $spool.PrinterPaused = $false; $spool.Pausable = $false
    Expect-Watch '' 1 'BVPRINT printer=failed detail=the printer cannot be paused' @("Add-PrinterPort $port", $add, 'Remove-Printer Mac (BridgeVM)') $false 'a printer that cannot be paused'
    $spool.Pausable = $true
    Reset-Model
    Expect-Watch 'off' 0 'BVPRINT printer=absent' @() $false 'off with nothing there'
    Reset-Model -Ports @($port) -Printers (& $printer 'Microsoft Print To PDF' $port)
    $script:fail['Remove-PrinterPort'] = 'The port is in use.'
    Expect-Watch 'off' 1 'BVPRINT printer=remove-failed detail=The port is in use.' @('Remove-Printer Mac (BridgeVM)', "Remove-PrinterPort $port") $false 'off, port still in use'
    Write-Output 'PASS: PRINTWATCH makes only what is missing, makes a wrong printer again, pauses it, reports a failure as one line with the watch off, removes a printer it cannot pause, and off removes printer and port'

    # 2. Job detection: the watch thread renames, TakePrintJobs announces.
    $watch = [BridgeVM.BvClipWatch]
    function Take { @($watch::TakePrintJobs()) }
    function Wait-Jobs([int]$Count, [string]$What, [int]$Seconds = 5) {
        $got = @()
        $end = [DateTime]::UtcNow.AddSeconds($Seconds)
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
    function Get-Pdf([int]$Length, [string]$Text = '') {
        $b = New-Object byte[] $Length
        [Text.Encoding]::ASCII.GetBytes('%PDF-1.7 ' + $Text).CopyTo($b, 0)
        return , $b
    }
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
    Assert ($watch::StartPrint($port, 'Mac (BridgeVM)')) 'watch restarted'
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
    [void]$watch::StartPrint($port, 'Mac (BridgeVM)')
    Start-Sleep -Milliseconds 600
    Assert ((Take).Count -eq 0 -and [IO.File]::Exists((Join-Path $other 'job.pdf'))) 'a linked folder is never read or renamed in'
    [IO.Directory]::Delete($dir)
    [IO.Directory]::Move($dir + '.real', $dir)
    [void]$watch::StartPrint($port, 'Mac (BridgeVM)')
    [void](Wait-Jobs 2 'the real folder again')
    foreach ($f in [IO.Directory]::GetFiles($dir)) { [IO.File]::Delete($f) }

    # Stopped: nothing is renamed or announced.
    $watch::StopPrint()
    Write-Job $port (Get-Pdf 13)
    Start-Sleep -Milliseconds 600
    Assert ([IO.File]::Exists($port) -and (Take).Count -eq 0) 'stopped'
    [IO.File]::Delete($port)
    Write-Output 'PASS: a finished job is renamed and announced once with its size, never while written, never through a reparse point, a restarted watch announces the rest, a stopped one nothing'

    # 3. The print gate, against the spooler model. Each job's document names it, so a job lost or
    # printed twice shows in the renamed files. Each case starts from a new model once the last case's
    # jobs have all left the queue and been renamed (on Windows a job's job.pdf can be renamed after the
    # job has left the queue), and waits for the jobs it expects with a deadline.
    function Wait-Until([scriptblock]$Done, [string]$What, [int]$Seconds = 30) {
        $end = [DateTime]::UtcNow.AddSeconds($Seconds)
        while (-not (& $Done) -and [DateTime]::UtcNow -lt $end) { Start-Sleep -Milliseconds 20 }
        Assert (& $Done) $What
    }
    function Get-Renamed { @([IO.Directory]::GetFiles($dir, '*.pdf') | Where-Object { [IO.Path]::GetFileName($_) -match '\A\d{19}\.pdf\z' } | Sort-Object) }
    function Get-Texts {
        (@(Get-Renamed | ForEach-Object {
            # The rename is visible before File.Move has released its Windows delete handle.
            # Read like GETRANGE, the real consumer, allowing that handle to finish closing.
            # The model closed its writer before the rename, so these bytes are complete.
            $file = [IO.File]::Open($_, [IO.FileMode]::Open, [IO.FileAccess]::Read,
                ([IO.FileShare]::ReadWrite -bor [IO.FileShare]::Delete))
            try {
                $bytes = New-Object byte[] $file.Length
                $offset = 0
                while ($offset -lt $bytes.Length) {
                    $read = $file.Read($bytes, $offset, $bytes.Length - $offset)
                    Assert ($read -gt 0) 'a renamed document is complete'
                    $offset += $read
                }
                ([Text.Encoding]::ASCII.GetString($bytes) -split "`0")[0]
            } finally { $file.Dispose() }
        }) -join '|')
    }
    # Model the delete-capable handle held while a Windows rename finishes. ReadAllBytes
    # denies delete sharing and fails here even though the writer has already closed.
    $renamed = Join-Path $dir '0000000000000000001.pdf'
    Write-Job $renamed (Get-Pdf 64 'rename-handle')
    $renameHandle = [IO.File]::Open($renamed, [IO.FileMode]::Open, [IO.FileAccess]::ReadWrite,
        ([IO.FileShare]::ReadWrite -bor [IO.FileShare]::Delete))
    try { Assert ((Get-Texts) -ceq '%PDF-1.7 rename-handle') 'read a complete renamed document with a shared handle still open' }
    finally { $renameHandle.Dispose(); [IO.File]::Delete($renamed) }
    # The renamed jobs are these documents, in this order, within 30 s; and none was lost.
    function Wait-Texts([string[]]$Names, [string]$What) {
        $want = @($Names | ForEach-Object { "%PDF-1.7 $_" }) -join '|'
        $end = [DateTime]::UtcNow.AddSeconds(30)
        while ((Get-Texts) -cne $want -and [DateTime]::UtcNow -lt $end) { Start-Sleep -Milliseconds 20 }
        $got = Get-Texts
        Assert ($got -ceq $want -and $spool.Lost -eq 0) "$What`: $got lost=$($spool.Lost)"
    }
    function Hold-Steady([scriptblock]$Same, [string]$What, [int]$Ms = 1200) {
        $end = [DateTime]::UtcNow.AddMilliseconds($Ms)
        while ([DateTime]::UtcNow -lt $end) { Assert (& $Same) $What; Start-Sleep -Milliseconds 20 }
    }
    $spooling = Join-Path $dir 'spooling.tmp'
    # A new model, its printer paused as the agent leaves it, and (unless -Away) the watch on it.
    function New-Spool([switch]$Away) {
        Wait-Until { $spool.Queued -eq 0 -and -not [IO.File]::Exists($port) -and -not [IO.File]::Exists($spooling) } 'the last case''s jobs all left the queue and were renamed'
        $spool.Stop()
        $watch::StopPrint()
        foreach ($f in Get-Renamed) { [IO.File]::Delete($f) }
        $script:spool = New-Object BridgeVM.SpoolerModel
        $spool.Port = $port
        $spool.PrinterPaused = $true
        [BridgeVM.SpoolerModel]::Install($spool)
        $spool.Start()
        if (-not $Away) { Assert ($watch::StartPrint($port, 'Mac (BridgeVM)')) 'watch on' }
    }

    # The watch starts with the printer paused, and the agent start pauses a printer left running.
    New-Spool -Away
    $spool.PrinterPaused = $false
    Assert ($watch::StartPrint($port, 'Mac (BridgeVM)') -and $spool.PrinterPaused) 'the watch start pauses the printer'
    $watch::StopPrint()
    $spool.PrinterPaused = $false
    $opened = [BridgeVM.SpoolerModel]::Opened; $closes = $spool.Closes
    $script:printerKnown = $false
    Suspend-BvPrinter
    Assert ([BridgeVM.SpoolerModel]::Opened -eq $opened -and -not $spool.PrinterPaused) 'no printer made here: the agent start opens nothing'
    $script:printerKnown = $true
    Suspend-BvPrinter
    Assert ($spool.PrinterPaused -and $spool.Closes -eq $closes + 1) 'the agent start pauses the printer it made, and lets go of it'
    $text = $ast.Extent.Text
    $hold = $text.IndexOf("`nSuspend-BvPrinter`r`n"); $open = $text.IndexOf('$h = Try-OpenPort')
    Assert ($hold -gt 0 -and $open -gt $hold) 'the agent start pauses the printer before it opens the port'
    # While the agent is away the paused printer keeps every job in its queue.
    [void]$spool.Add((Get-Pdf 400 'away-1')); [void]$spool.Add((Get-Pdf 400 'away-2'))
    Hold-Steady { $spool.Printed -eq 0 -and $spool.Queued -eq 2 } 'jobs wait in the paused queue while no watch runs' 600
    # They go out one at a time once the watch runs, oldest first, none lost.
    Assert ($watch::StartPrint($port, 'Mac (BridgeVM)')) 'watch on'
    Wait-Texts @('away-1', 'away-2') 'the jobs queued while away'
    Wait-Until { $spool.PrinterPaused -and $spool.PausedJobs -eq 0 } 'idle again: the printer paused, no job paused'

    # A second job is not let out until the first one's job.pdf is renamed (here a link, which the
    # watch never renames, stands for a job.pdf not renamed yet).
    New-Spool
    [void](New-Item -ItemType SymbolicLink -Path $port -Value $secret)
    [void]$spool.Add((Get-Pdf 400 'held'))
    Hold-Steady { $spool.Printed -eq 0 -and $spool.PrinterPaused } 'no job goes out while job.pdf holds one'
    [IO.File]::Delete($port)
    Wait-Texts @('held') 'out once job.pdf is renamed'

    # At MaxPrintJobs renamed jobs waiting for the host the printer simply stays paused.
    New-Spool
    $max = $watch::MaxPrintJobs
    for ($i = 0; $i -lt $max; $i++) { Write-Job (Join-Path $dir ('{0:D19}.pdf' -f (100 + $i))) (Get-Pdf 64 "old-$i") }
    [void]$spool.Add((Get-Pdf 400 'cap-1')); [void]$spool.Add((Get-Pdf 400 'cap-2'))
    Hold-Steady { $spool.Printed -eq 0 -and $spool.PrinterPaused -and $spool.Queued -eq 2 } 'at the cap nothing is let out'
    [IO.File]::Delete((Get-Renamed)[0])
    Wait-Until { $spool.Printed -eq 1 } 'one out once the host took one'
    Hold-Steady { $spool.Printed -eq 1 -and $spool.Queued -eq 1 } 'the cap again: the next one waits'
    [IO.File]::Delete((Get-Renamed)[0])
    Wait-Texts @((2..($max - 1) | ForEach-Object { "old-$_" }) + 'cap-1', 'cap-2') 'cap'

    # A burst of five that print in no time: without the others paused the spooler would print them
    # back to back into job.pdf faster than the watch renames it.
    New-Spool
    $spool.PrintMs = 0
    foreach ($n in 1..5) { [void]$spool.Add((Get-Pdf 600 "burst-$n")) }
    Wait-Texts @(1..5 | ForEach-Object { "burst-$_" }) 'burst'
    Assert ($spool.StartOrder -ceq '1,2,3,4,5') "burst: started $($spool.StartOrder)"

    # Jobs that arrive while one is out wait behind it.
    New-Spool
    foreach ($n in 1..2) { [void]$spool.Add((Get-Pdf 600 "wave-$n")) }
    Wait-Until { $spool.StartOrder -cne '' } 'the first of the wave out'
    foreach ($n in 3..5) { [void]$spool.Add((Get-Pdf 600 "wave-$n")); Start-Sleep -Milliseconds 15 }
    Wait-Texts @(1..5 | ForEach-Object { "wave-$_" }) 'wave'
    Wait-Until { $spool.PrinterPaused -and $spool.PausedJobs -eq 0 -and $spool.Queued -eq 0 } 'idle again after the wave'

    # A printer someone resumed by hand is paused again within half a second.
    New-Spool
    $spool.PrinterPaused = $false
    Wait-Until { $spool.PrinterPaused } 'a printer resumed by hand is paused again' 3
    [void]$spool.Add((Get-Pdf 600 'resumed-1')); [void]$spool.Add((Get-Pdf 600 'resumed-2'))
    Wait-Texts @('resumed-1', 'resumed-2') 'after the printer was resumed by hand'
    Write-Output 'PASS: the printer is paused at watch and agent start, one job goes out at a time, none while job.pdf holds one, none at MaxPrintJobs; bursts and arrivals all arrive, none lost'
} finally {
    [BridgeVM.BvClipWatch]::StopPrint()
    $spool.Stop()
    Remove-Item -LiteralPath $work -Recurse -Force -ErrorAction SilentlyContinue
}

& (Get-Process -Id $PID).Path -NoProfile -File $PSCommandPath -Mode readline
if ($LASTEXITCODE -ne 0) { throw 'Read-Line dispatch failed' }
Write-Output 'PASS: the agent printer to the Mac (setup, job detection, print gate, PRINTJOB dispatch)'
