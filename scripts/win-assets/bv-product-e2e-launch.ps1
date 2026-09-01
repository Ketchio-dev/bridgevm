[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [ValidateSet('KeyboardPointer', 'Clipboard', 'Share', 'Network', 'Audio', 'MarkerA', 'MarkerB', 'MarkerRestoredA', 'AgentResult')]
    [string]$Action,
    [Parameter(Mandatory = $true)]
    [ValidatePattern('^[0-9a-f]{64}$')]
    [string]$Nonce,
    [string]$JobID = '',
    [string]$Commit = '',
    [int]$Lane = 0,
    [string]$VMSlug = ''
)

$ErrorActionPreference = 'Stop'
$Prefix = $Nonce.Substring(0, 12)
$Arguments = @(
    '-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', 'C:\bridgevm-share\bv-product-e2e.ps1',
    '-Action', $Action, '-Nonce', $Nonce
)
if ($Action -eq 'AgentResult') {
    $Arguments += @('-JobID', $JobID, '-Commit', $Commit, '-Lane', [string]$Lane, '-VMSlug', $VMSlug)
}
$CommandLine = 'powershell.exe ' + ($Arguments -join ' ')
$Result = Invoke-CimMethod -ClassName Win32_Process -MethodName Create -Arguments @{ CommandLine = $CommandLine }
if ($Result.ReturnValue -ne 0 -or $Result.ProcessId -le 0) { throw 'CIM workload launch failed' }
Write-Output ("T17-LAUNCHED-$Action-$Prefix pid=" + $Result.ProcessId)
exit 0
