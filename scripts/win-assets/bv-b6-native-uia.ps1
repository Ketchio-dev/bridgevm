param([long]$Hwnd)
$ErrorActionPreference = 'Stop'
$result = [ordered]@{
    schema_version = 'b6.native-uia-observation.v1'
    observation_only = $true
    criterion_pass = $false
    hwnd = [string]$Hwnd
    apartment = [Threading.Thread]::CurrentThread.GetApartmentState().ToString()
    is_64bit_process = [Environment]::Is64BitProcess
    status = 'started'
    stage = 'compile-interop'
    query = $null
    exception_chain = @()
}
try {
    Add-Type -Path (Join-Path $PSScriptRoot 'bv-b6-native-uia.cs')
    $result.query = [BridgeVmNativeUia.Probe]::Query($Hwnd)
    $result.status = $result.query.status
    $result.stage = $result.query.stage
} catch {
    $result.status = 'query-failed'
    $cause = $_.Exception
    while ($null -ne $cause -and $result.exception_chain.Count -lt 8) {
        $result.exception_chain += @{ type = $cause.GetType().FullName; hresult = $cause.HResult }
        $cause = $cause.InnerException
    }
}
Write-Output ('BVNATIVEUIAPROBE ' + (ConvertTo-Json -InputObject $result -Depth 7 -Compress))
if ($result.status -ne 'query-returned') { exit 1 }
exit 0
