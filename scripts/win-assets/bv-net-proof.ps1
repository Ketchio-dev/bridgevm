[CmdletBinding()]
param()

# D1 NET-LIVE proof: prove live guest egress through the BridgeVM userspace NAT.
# Every probe prints a single grep-able RESULT line; the agent OUT reply carries
# these into run.log. No secrets, only public endpoints (example.com, 1.1.1.1).

$ErrorActionPreference = 'Continue'
$ProgressPreference = 'SilentlyContinue'

Write-Output ('NETPROOF begin utc=' + [DateTime]::UtcNow.ToString('o'))

# 1. DNS resolution.
$dns = 'FAIL'
try {
  $r = Resolve-DnsName -Name 'example.com' -Type A -ErrorAction Stop
  $ip = ($r | Where-Object { $_.IPAddress } | Select-Object -First 1).IPAddress
  if ($ip) { $dns = 'OK ' + $ip }
} catch { $dns = 'FAIL ' + $_.Exception.Message }
Write-Output ('NETPROOF dns=' + $dns)

# 2. HTTP GET with explicit status code.
$http = 'FAIL'
try {
  $resp = Invoke-WebRequest -Uri 'http://example.com/' -UseBasicParsing -TimeoutSec 30
  $http = 'OK StatusCode : ' + [int]$resp.StatusCode
} catch {
  if ($_.Exception.Response) {
    $http = 'HTTPERR StatusCode : ' + [int]$_.Exception.Response.StatusCode
  } else {
    $http = 'FAIL ' + $_.Exception.Message
  }
}
Write-Output ('NETPROOF http=' + $http)

# 3. ICMP echo.
$icmp = 'FAIL'
try {
  $ok = Test-Connection -ComputerName '1.1.1.1' -Count 2 -Quiet -ErrorAction Stop
  if ($ok) { $icmp = 'OK' } else { $icmp = 'FAIL no-reply' }
} catch { $icmp = 'FAIL ' + $_.Exception.Message }
Write-Output ('NETPROOF icmp=' + $icmp)

# 4. Adapter identity for the receipt.
try {
  $nic = Get-NetIPConfiguration -ErrorAction Stop | Where-Object { $_.IPv4Address } | Select-Object -First 1
  if ($nic) {
    Write-Output ('NETPROOF adapter=' + $nic.InterfaceAlias +
      ' ipv4=' + ($nic.IPv4Address.IPAddress -join ',') +
      ' gw=' + ($nic.IPv4DefaultGateway.NextHop -join ','))
  }
} catch { Write-Output ('NETPROOF adapter=FAIL ' + $_.Exception.Message) }

$pass = ($dns -like 'OK*') -and ($http -like 'OK*') -and ($icmp -like 'OK*')
Write-Output ('NETPROOF verdict=' + (@{ $true = 'PASS'; $false = 'FAIL' }[$pass]))
Write-Output ('NETPROOF end utc=' + [DateTime]::UtcNow.ToString('o'))
exit 0
