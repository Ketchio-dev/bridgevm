# The guest half of scripts/live-gates/hda-microphone-live-check.py: lists the capture
# endpoints Windows created, then captures the default one for -Seconds in WASAPI
# exclusive mode and checks the host's index pattern (bv-mic-capture.cs).
param([ValidateRange(1, 10)][int]$Seconds = 3)

$ErrorActionPreference = 'Stop'
$capture = @(Get-PnpDevice -Class AudioEndpoint -PresentOnly |
    Where-Object { $_.InstanceId -like 'SWD\MMDEVAPI\{0.0.1.*' })
foreach ($endpoint in $capture) {
    $name = [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes([string]$endpoint.FriendlyName))
    Write-Output ('MIC ENDPOINT status=' + $endpoint.Status + ' name=' + $name)
}
Write-Output ('MIC ENDPOINTS capture=' + $capture.Count)
try {
    # Windows PowerShell 5.1 compiles with the .NET Framework's C# 5 compiler; its Add-Type has no
    # -CompilerOptions (PowerShell 7 only).
    Add-Type -Path (Join-Path $PSScriptRoot 'bv-mic-capture.cs')
    Write-Output ([BvMicCapture]::Run($Seconds))
} catch {
    $message = $_.Exception.Message
    if ($_.Exception.InnerException) { $message = $_.Exception.InnerException.Message }
    if (-not $message.StartsWith('MIC ERROR')) { $message = 'MIC ERROR ' + $message }
    Write-Output ($message -replace '[\r\n]+', ' ')
    exit 1
}
