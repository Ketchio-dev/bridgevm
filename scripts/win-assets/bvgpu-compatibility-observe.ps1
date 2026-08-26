[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$Candidates,
    [Parameter(Mandatory = $true)][string]$OutDir,
    [int]$WarmupSeconds = 5,
    [int]$MeasureSeconds = 30
)

$ErrorActionPreference = 'Stop'
$ExpectedHeader = "id`tpackage`tapp_id`texecutable`texecutable_sha256`tblockmap_sha256`tversion`tstatic_graphics_imports"
$AllowedModules = @('d3d11.dll','d3d12.dll','dxgi.dll','opengl32.dll','vulkan-1.dll','vulkan_virtio.dll')

function Hash-Lower([string]$Path) {
    (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
}

function Fail-Identity([string]$Id, [string]$Reason) {
    Write-Output "BV-COMPAT identity-failure id=$Id reason=$Reason"
    throw "identity-failure-$Id-$Reason"
}

if (-not (Test-Path -LiteralPath $Candidates -PathType Leaf)) { throw 'candidate-file-missing' }
if (-not (Test-Path -LiteralPath $OutDir)) { [void](New-Item -ItemType Directory -Path $OutDir -Force) }
$raw = [IO.File]::ReadAllText($Candidates).Replace("`r`n", "`n")
$header = $raw.Split("`n")[0]
if ($header -cne $ExpectedHeader) { throw 'candidate-header-mismatch' }
$rows = @(Import-Csv -LiteralPath $Candidates -Delimiter "`t")
if ($rows.Count -ne 20 -or @($rows.id | Sort-Object -Unique).Count -ne 20) { throw 'candidate-count-mismatch' }

$result = [System.Collections.Generic.List[string]]::new()
$result.Add("id`tidentity_verified`tlaunch`tvisible`tclean_shutdown`tsamples`tseries_sha256`tloaded_modules`treason")
$frameTool = Join-Path (Split-Path -Parent $Candidates) 'bvgpu-frametime-series.ps1'
if (-not (Test-Path -LiteralPath $frameTool -PathType Leaf)) { throw 'frametime-tool-missing' }

foreach ($row in $rows) {
    $id = [string]$row.id
    Write-Output "BV-COMPAT begin id=$id"
    $package = Get-AppxPackage -AllUsers | Where-Object { $_.PackageFullName -ceq $row.package } | Select-Object -First 1
    if (-not $package) { Fail-Identity $id 'package-missing' }
    if ($package.Version.ToString() -cne $row.version) { Fail-Identity $id 'version-mismatch' }
    $blockMap = Join-Path $package.InstallLocation 'AppxBlockMap.xml'
    if (-not (Test-Path -LiteralPath $blockMap -PathType Leaf) -or (Hash-Lower $blockMap) -cne $row.blockmap_sha256) {
        Fail-Identity $id 'blockmap-mismatch'
    }
    $relative = ([string]$row.executable).Replace('/', '\')
    $executable = Join-Path $package.InstallLocation $relative
    if (-not (Test-Path -LiteralPath $executable -PathType Leaf) -or (Hash-Lower $executable) -cne $row.executable_sha256) {
        Fail-Identity $id 'executable-mismatch'
    }

    $processName = [IO.Path]::GetFileNameWithoutExtension($relative)
    Get-Process -Name $processName -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
    Start-Sleep -Milliseconds 500
    $aumid = $package.PackageFamilyName + '!' + $row.app_id
    $launch = 'explorer.exe shell:AppsFolder\' + $aumid
    $created = Invoke-CimMethod -ClassName Win32_Process -MethodName Create -Arguments @{CommandLine=$launch}
    $target = $null
    $deadline = (Get-Date).AddSeconds(45)
    while ((Get-Date) -lt $deadline) {
        $target = Get-Process -Name $processName -ErrorAction SilentlyContinue | Sort-Object StartTime | Select-Object -Last 1
        if ($target) { break }
        Start-Sleep -Milliseconds 500
    }
    $launched = if ($target) { 'yes' } else { 'no' }
    $visible = 'no'
    $moduleSet = [System.Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
    if ($target) {
        try {
            $target.Refresh()
            if ($target.MainWindowHandle -ne 0) { $visible = 'yes' }
            foreach ($module in $target.Modules) {
                $name = $module.ModuleName.ToLowerInvariant()
                if ($AllowedModules -contains $name) { [void]$moduleSet.Add($name) }
            }
        } catch { Write-Output "BV-COMPAT module-enum-refused id=$id" }
    }

    $series = Join-Path $OutDir "$id.frametimes-ms"
    Remove-Item -LiteralPath $series -Force -ErrorAction SilentlyContinue
    if ($target) {
        & $frameTool -Id $id -ProcessName $processName -OutDir $OutDir -WarmupSeconds $WarmupSeconds -MeasureSeconds $MeasureSeconds
        $target = Get-Process -Name $processName -ErrorAction SilentlyContinue | Select-Object -First 1
        if ($target) {
            try {
                $target.Refresh()
                if ($target.MainWindowHandle -ne 0) { $visible = 'yes' }
                foreach ($module in $target.Modules) {
                    $name = $module.ModuleName.ToLowerInvariant()
                    if ($AllowedModules -contains $name) { [void]$moduleSet.Add($name) }
                }
            } catch { Write-Output "BV-COMPAT module-enum-refused-after id=$id" }
        }
    }

    $samples = 0; $seriesHash = 'absent'
    if (Test-Path -LiteralPath $series -PathType Leaf) {
        $samples = @([IO.File]::ReadAllLines($series) | Where-Object { $_.Trim() }).Count
        if ($samples -gt 0) { $seriesHash = Hash-Lower $series }
    }
    $clean = 'no'
    if ($target) {
        try { [void]$target.CloseMainWindow() } catch { }
        try { Wait-Process -Id $target.Id -Timeout 15 -ErrorAction Stop; $clean = 'yes' } catch {
            Stop-Process -Id $target.Id -Force -ErrorAction SilentlyContinue
            try { Wait-Process -Id $target.Id -Timeout 10 -ErrorAction Stop } catch { }
        }
    }
    $modules = @($moduleSet | Sort-Object)
    $moduleText = if ($modules.Count) { $modules -join ',' } else { 'none' }
    $reason = if (-not $target -and $launched -eq 'no') { 'process-not-running' } elseif ($samples -eq 0) { 'no-frame-series' } else { 'completed' }
    $result.Add("$id`tyes`t$launched`t$visible`t$clean`t$samples`t$seriesHash`t$moduleText`t$reason")
    Write-Output "BV-COMPAT end id=$id launch=$launched visible=$visible samples=$samples modules=$moduleText"
}

[IO.File]::WriteAllText((Join-Path $OutDir 'observations.tsv'), ($result -join "`n") + "`n")
[IO.File]::WriteAllText((Join-Path $OutDir 'compatibility-observation.done'), "rows=20`n")
Write-Output 'BV-COMPAT-DONE rows=20'
