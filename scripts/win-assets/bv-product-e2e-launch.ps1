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

$ErrorActionPreference = 'Stop'; $Prefix = $Nonce.Substring(0, 12)
$Arguments = @(
    '-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', 'C:\bridgevm-share\bv-product-e2e.ps1',
    '-Action', $Action, '-Nonce', $Nonce
)
if ($Action -eq 'AgentResult') {
    $Arguments += @('-JobID', $JobID, '-Commit', $Commit, '-Lane', [string]$Lane, '-VMSlug', $VMSlug)
}
$Result = Invoke-CimMethod -ClassName Win32_Process -MethodName Create -Arguments @{ CommandLine = 'powershell.exe ' + ($Arguments -join ' ') }
if ($Result.ReturnValue -ne 0 -or $Result.ProcessId -le 0) { throw 'CIM workload launch failed' }
Write-Output ("T17-LAUNCHED-$Action-$Prefix pid=" + $Result.ProcessId)
# The launcher shares the agent's token, so this is the injecting side's session and integrity level.
if ($Action -eq 'KeyboardPointer') { $Integrity = switch -Regex ((whoami /groups) -join ' ') { 'S-1-16-16384' { 'system'; break } 'S-1-16-12288' { 'high'; break } 'S-1-16-8192' { 'medium'; break } 'S-1-16-4096' { 'low'; break } default { 'unknown' } }; Write-Output ("T17-CONTEXT launcher-session=$([Diagnostics.Process]::GetCurrentProcess().SessionId) launcher-integrity=$Integrity workload-session=" + (Get-CimInstance Win32_Process -Filter "ProcessId=$($Result.ProcessId)").SessionId) }
exit 0
