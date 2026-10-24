[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [ValidateSet('BitLockerEnable', 'BitLockerStatus')]
    [string]$Action,
    [Parameter(Mandatory = $true)]
    [ValidatePattern('^[0-9a-f]{64}$')]
    [string]$Nonce
)
# Starts the T19 BitLocker workload as bv-product-e2e-launch.ps1 starts the T17 workloads: through
# Win32_Process Create, so it outlives the agent command, and with the nonce-bound marker the host waits for.
$ErrorActionPreference = 'Stop'; $Prefix = $Nonce.Substring(0, 12)
$Arguments = @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', 'C:\bridgevm-share\bv-product-e2e-bitlocker.ps1', '-Action', $Action, '-Nonce', $Nonce)
$Result = Invoke-CimMethod -ClassName Win32_Process -MethodName Create -Arguments @{ CommandLine = 'powershell.exe ' + ($Arguments -join ' ') }
if ($Result.ReturnValue -ne 0 -or $Result.ProcessId -le 0) { throw 'CIM workload launch failed' }
Write-Output ("T19-BITLOCKER-LAUNCHED-$Action-$Prefix pid=" + $Result.ProcessId)
exit 0
