# Diagnostic collector only. CSV collection is not a B6 criterion pass.
# Host staging must authenticate the pinned PresentMon binary before execution.
param(
    [Parameter(Mandatory=$true)][string]$PresentMonPath,
    [Parameter(Mandatory=$true)][string]$OutputCsvPath,
    [ValidateRange(1,300)][int]$Seconds = 20
)
$ErrorActionPreference = 'Stop'
$binary = Get-Item -LiteralPath $PresentMonPath -ErrorAction Stop
if ($binary.PSIsContainer -or ($binary.Attributes -band [IO.FileAttributes]::ReparsePoint)) {
    throw 'PresentMon binary must be a regular file'
}
$output = [IO.Path]::GetFullPath($OutputCsvPath)
if ($output -match '["\r\n]' -or [IO.Path]::GetExtension($output) -ine '.csv') {
    throw 'PresentMon output must be a CSV path without quote or line-break characters'
}
$parent = Get-Item -LiteralPath ([IO.Path]::GetDirectoryName($output)) -ErrorAction Stop
if (-not $parent.PSIsContainer) { throw 'PresentMon output parent is not a directory' }
if (Test-Path -LiteralPath $output) { throw 'PresentMon output already exists; preserve it and use a new path' }
# Never terminate another collector's default ETW session.
$session = 'BridgeVM-B6-' + [Guid]::NewGuid().ToString('N')
# Start-Process joins array elements with spaces; preserve the native quotes.
$arguments = '--process_name dwm.exe --timed ' + $Seconds +
    ' --terminate_after_timed --v2_metrics --session_name "' + $session +
    '" --output_file "' + $output + '"'
$process = $null
try {
    $process = Start-Process -FilePath $binary.FullName -ArgumentList $arguments -PassThru -WindowStyle Hidden
    if (-not $process.WaitForExit(($Seconds + 30) * 1000)) {
        throw 'PresentMon capture did not terminate in time'
    }
    $process.Refresh()
    if ($process.ExitCode -ne 0) { throw "PresentMon exited with code $($process.ExitCode)" }
    $csv = Get-Item -LiteralPath $output -ErrorAction Stop
    if ($csv.PSIsContainer -or ($csv.Attributes -band [IO.FileAttributes]::ReparsePoint) -or
        $csv.Length -le 0 -or $csv.Length -gt 7500000) {
        throw 'PresentMon CSV is not a regular nonempty file within the shared-file size bound'
    }
    $rows = @(Import-Csv -LiteralPath $output -ErrorAction Stop)
    if ($rows.Count -eq 0) { throw 'PresentMon produced no frame rows' }
    if (-not ($rows[0].PSObject.Properties.Name -contains 'FrameTime') -or
        -not ($rows[0].PSObject.Properties.Name -contains 'Application')) {
        throw 'PresentMon CSV is missing the required v2 frame columns'
    }
    $digest = (Get-FileHash -LiteralPath $output -Algorithm SHA256).Hash.ToLowerInvariant()
    Write-Output "BVPRESENTMON path=$output rows=$($rows.Count + 1) data_rows=$($rows.Count) sha256=$digest session=$session"
} finally {
    if ($null -ne $process) {
        try {
            if (-not $process.HasExited) {
                $process.Kill()
                if (-not $process.WaitForExit(5000)) { throw 'Owned PresentMon process did not exit after cancellation' }
            }
        } finally {
            $process.Dispose()
        }
    }
}
