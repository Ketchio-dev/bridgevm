param([string]$Query, [long]$Hwnd, [int]$Width, [int]$Height, [switch]$ExpectedNotFound)
$ErrorActionPreference = 'Stop'
$global:LASTEXITCODE = 0
$result = & $Query -Hwnd $Hwnd -Width $Width -Height $Height
$queryExitCode = $global:LASTEXITCODE
$result | Write-Output
if ($queryExitCode -ne 0) { exit $queryExitCode }
if (!$ExpectedNotFound -and $result -match 'state=not-found') {
    & (Join-Path $PSScriptRoot 'b6-tip-provider-tree.ps1') -Hwnd $Hwnd
}
exit $queryExitCode
