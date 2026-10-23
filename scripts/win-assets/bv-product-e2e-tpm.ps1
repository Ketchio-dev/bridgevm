[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [ValidateSet('TpmBoot1', 'TpmBoot2', 'TpmBoot3')]
    [string]$Action,
    [Parameter(Mandatory = $true)]
    [ValidatePattern('^[0-9a-f]{64}$')]
    [string]$Nonce
)
# One T17 boot's vTPM observation: Get-Tpm's TpmPresent and TpmReady, and PCR 0 and PCR 7
# (SHA-256 bank) read through TBS by bv-product-e2e-tpm.cs. Only fixed codes, booleans and
# SHA-256 hashes of the PCR values reach the share, in one result of at most 512 bytes
# that is staged outside the share and renamed in whole. A failure here is a recorded
# code, never the shared T17 error file, so it cannot be mistaken for a failed workload.
$ErrorActionPreference = 'Stop'
$Share = 'C:\bridgevm-share'; $Boot = $Action.Substring(7); $Prefix = $Nonce.Substring(0, 12)
$Present = 'unknown'; $Ready = 'unknown'
try {
    $Tpm = Get-Tpm
    if ($Tpm.TpmPresent -is [bool]) { $Present = $Tpm.TpmPresent.ToString().ToLowerInvariant() }
    if ($Tpm.TpmReady -is [bool]) { $Ready = $Tpm.TpmReady.ToString().ToLowerInvariant() }
} catch { $Present = 'unknown'; $Ready = 'unknown' }
try {
    Add-Type -Path (Join-Path $Share 'bv-product-e2e-tpm.cs')
    $Reading = [BridgeVM.BvTpmPcr]::Read()
    $Read = $Reading.Status; $Pcr0 = $Reading.Pcr0Sha256; $Pcr7 = $Reading.Pcr7Sha256
} catch { $Read = 'helper-failed'; $Pcr0 = 'absent'; $Pcr7 = 'absent' }
$Body = "bridgevm-t17-tpm-pcr-v1`nnonce=$Nonce`nboot=$Boot`ntpm_present=$Present`ntpm_ready=$Ready`npcr_read=$Read`npcr0_sha256=$Pcr0`npcr7_sha256=$Pcr7`n"
New-Item -ItemType Directory -Force -Path 'C:\ProgramData\BridgeVM' | Out-Null
$Staged = "C:\ProgramData\BridgeVM\t17-tpm-boot$Boot.tmp"
[IO.File]::WriteAllBytes($Staged, (New-Object System.Text.UTF8Encoding($false)).GetBytes($Body))
Move-Item -LiteralPath $Staged -Destination (Join-Path $Share "t17-tpm-boot$Boot-$Prefix.txt") -Force
Write-Output ("T17 action=$Action nonce=$Nonce status=PASS")
exit 0
