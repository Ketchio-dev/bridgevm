# Native Windows contract test. The fixture is not PresentMon or live evidence.
$ErrorActionPreference = 'Stop'
$root = Join-Path ([IO.Path]::GetTempPath()) ('BridgeVM B6 collector ' + [Guid]::NewGuid().ToString('N'))
$null = New-Item -ItemType Directory -Path $root
$collector = Join-Path $PSScriptRoot '../../scripts/win-assets/bv-b6-presentmon-capture.ps1'
$binary = Join-Path $root 'collector fixture.exe'
$source = @'
using System;
using System.Diagnostics;
using System.IO;
using System.Threading;
public static class CollectorFixture {
    public static int Main(string[] args) {
        string output = null, session = null;
        for (int i = 0; i < args.Length; ++i) {
            switch (args[i]) {
            case "--process_name": if (++i >= args.Length || args[i] != "dwm.exe") return 21; break;
            case "--timed": if (++i >= args.Length) return 22; break;
            case "--output_file": if (++i >= args.Length) return 23; output = args[i]; break;
            case "--session_name": if (++i >= args.Length) return 24; session = args[i]; break;
            case "--terminate_after_timed": case "--v2_metrics": break;
            default: return 25;
            }
        }
        if (output == null || session == null || !session.StartsWith("BridgeVM-B6-")) return 26;
        File.WriteAllText(output + ".pid", Process.GetCurrentProcess().Id.ToString());
        File.WriteAllText(output + ".session", session);
        string mode = Path.GetFileName(output);
        if (mode.StartsWith("hang")) Thread.Sleep(60000);
        if (mode.StartsWith("failure")) return 7;
        if (mode.StartsWith("missing")) return 0;
        if (mode.StartsWith("oversized")) { File.WriteAllText(output, new string('x', 7500001)); return 0; }
        if (mode.StartsWith("wrong-columns")) { File.WriteAllText(output, "other\r\n1\r\n"); return 0; }
        string csv = "Application,ProcessID,SwapChainAddress,CPUStartTime,FrameTime\r\n";
        if (!mode.StartsWith("empty")) csv += "dwm.exe,10,0x20,0,16.6667\r\ndwm.exe,10,0x20,16.6667,16.6667\r\n";
        File.WriteAllText(output, csv);
        return 0;
    }
}
'@
$script:checks = 0
function Assert-True([bool]$Condition, [string]$Message) {
    if (-not $Condition) { throw $Message }
    $script:checks++
}
function Assert-Refused([string]$Name, [int]$Duration = 1) {
    $path = Join-Path $root $Name
    $refused = $false
    try { $null = & $collector -PresentMonPath $binary -OutputCsvPath $path -Seconds $Duration }
    catch { $refused = $true }
    Assert-True $refused ("Expected refusal: " + $Name)
}
try {
    Add-Type -TypeDefinition $source -OutputAssembly $binary -OutputType ConsoleApplication
    $good = Join-Path $root 'good output with spaces.csv'
    $receipt = & $collector -PresentMonPath $binary -OutputCsvPath $good -Seconds 1
    Assert-True (Test-Path -LiteralPath $good) 'Quoted output path did not arrive intact'
    Assert-True ($receipt -match 'data_rows=2 sha256=[0-9a-f]{64} session=BridgeVM-B6-') 'Missing diagnostic identity'
    $second = Join-Path $root 'second output.csv'
    $null = & $collector -PresentMonPath $binary -OutputCsvPath $second -Seconds 1
    Assert-True ((Get-Content -LiteralPath ($good + '.session')) -ne
                 (Get-Content -LiteralPath ($second + '.session'))) 'Collectors shared an ETW session identity'
    $stale = Join-Path $root 'stale.csv'
    [IO.File]::WriteAllText($stale, 'retained prior evidence')
    Assert-Refused 'stale.csv'
    Assert-True ([IO.File]::ReadAllText($stale) -eq 'retained prior evidence') 'Prior CSV was changed'
    Assert-True (-not (Test-Path -LiteralPath ($stale + '.pid'))) 'Collector started despite existing output'
    foreach ($name in @('empty.csv', 'failure.csv', 'missing.csv', 'oversized.csv', 'wrong-columns.csv')) {
        Assert-Refused $name
    }
    Assert-Refused 'invalid-duration.csv' 0
    Assert-Refused 'invalid-duration-high.csv' 301
    Assert-Refused 'invalid-extension.txt'
    Assert-Refused 'invalid"quote.csv'
    Assert-Refused 'missing-parent/output.csv'
    Assert-Refused 'hang.csv'
    $hungPid = [int](Get-Content -LiteralPath (Join-Path $root 'hang.csv.pid'))
    Assert-True ($null -eq (Get-Process -Id $hungPid -ErrorAction SilentlyContinue)) 'Owned timed-out process survived'
    Write-Output "B6 collector native contracts: PASS ($script:checks checks; fixture only, no live frame claim)"
} finally {
    Remove-Item -LiteralPath $root -Recurse -Force
}
