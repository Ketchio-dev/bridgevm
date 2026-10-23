# The Windows PowerShell job of .github/workflows/t17-tpm-pcr-contract.yml runs each guest
# PowerShell contract here in turn and stops at the first one that throws or exits non-zero.
$ErrorActionPreference = 'Stop'
foreach ($Contract in 't17-tpm-pcr-contract.ps1', 't19-bitlocker-guest-contract.ps1') {
    & (Join-Path $PSScriptRoot $Contract)
    if ($LASTEXITCODE -ne 0) { throw "$Contract exited with $LASTEXITCODE" }
}
exit 0
