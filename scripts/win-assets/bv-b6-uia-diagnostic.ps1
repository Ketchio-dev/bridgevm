param([long]$Hwnd, [int]$Width, [int]$Height)
$ErrorActionPreference = 'Stop'
if ($Hwnd -le 0 -or $Width -le 1 -or $Height -le 1) { throw 'Invalid diagnostic target' }
$result = [ordered]@{
    schema_version = 'b6.uia-apartment-observation.v1'
    observation_only = $true
    criterion_pass = $false
    hwnd = [string]$Hwnd
    apartment = [Threading.Thread]::CurrentThread.GetApartmentState().ToString()
    is_64bit_process = [Environment]::Is64BitProcess
    powershell_version = $PSVersionTable.PSVersion.ToString()
    status = 'started'
    query_output = @()
    exception_chain = @()
}
try {
    $result.query_output = @(& (Join-Path $PSScriptRoot 'bv-b6-tip-point.ps1') -Hwnd $Hwnd -Width $Width -Height $Height)
    $result.status = 'query-returned'
} catch {
    $result.status = 'query-failed'
    $cause = $_.Exception
    while ($null -ne $cause -and $result.exception_chain.Count -lt 8) {
        $result.exception_chain += @{ type = $cause.GetType().FullName; hresult = $cause.HResult }
        $cause = $cause.InnerException
    }
}
Write-Output ('BVUIAPROBE ' + (ConvertTo-Json -InputObject $result -Depth 5 -Compress))
if ($result.status -ne 'query-returned') { exit 1 }
