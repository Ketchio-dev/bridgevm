[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [ValidateSet("Certificates", "DriverStore", "VerifyClean", "VerifyInstalled")]
    [string]$Phase,

    [string]$PackageDirectory = "C:\BridgeVM\viogpu3d",
    [string]$LogPath = "C:\BridgeVM\viogpu3d-cleanup.log"
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$expectedInf = Join-Path $PackageDirectory "viogpu3d.inf"
$expectedCer = Join-Path $PackageDirectory "BridgeVM-viogpu3d-Test.cer"
$displayClassGuid = "{4d36e968-e325-11ce-bfc1-08002be10318}"
$driverStoreRoot = Join-Path $env:windir "System32\DriverStore\FileRepository"
$windowsInfRoot = Join-Path $env:windir "INF"

function Write-CleanupLog {
    param([Parameter(Mandatory = $true)][string]$Message)

    $line = "[bvgpu-clean] phase=$Phase $Message"
    Write-Output $line
    [IO.File]::AppendAllText($LogPath, $line + [Environment]::NewLine)
}

function Get-ExpectedCertificate {
    if (-not (Test-Path -LiteralPath $expectedCer -PathType Leaf)) {
        throw "Expected signing certificate is missing: $expectedCer"
    }
    return [Security.Cryptography.X509Certificates.X509Certificate2]::new($expectedCer)
}

function Get-VioGpu3DPublishedInfs {
    # Do not call this variable $matches: PowerShell variable names are case
    # insensitive, so every -match expression would replace it with the
    # automatic $Matches hash table.
    $publishedMatches = @()
    foreach ($inf in @(Get-ChildItem -LiteralPath $windowsInfRoot -Filter "oem*.inf" -File -ErrorAction Stop)) {
        $text = [IO.File]::ReadAllText($inf.FullName)
        $isDisplay = $text -match ('(?im)^\s*ClassGuid\s*=\s*' + [regex]::Escape($displayClassGuid) + '\s*$')
        $isVioGpu3D = $text -match '(?im)^\s*ServiceBinary\s*=.*\\viogpu3d[.]sys\s*$' -or
            $text -match '(?im)^\s*AddService\s*=\s*VioGpu3D\s*,'
        if ($isDisplay -and $isVioGpu3D) {
            $publishedMatches += $inf
        }
    }
    return @($publishedMatches)
}

function Get-VioGpu3DRepositoryDirectories {
    if (-not (Test-Path -LiteralPath $driverStoreRoot -PathType Container)) {
        throw "DriverStore FileRepository is missing: $driverStoreRoot"
    }
    return @(Get-ChildItem -LiteralPath $driverStoreRoot -Directory -Filter "viogpu3d.inf_*" -ErrorAction Stop)
}

function Remove-OldCertificates {
    $expected = Get-ExpectedCertificate
    $expectedThumbprint = $expected.Thumbprint.ToUpperInvariant()
    $expectedSubject = $expected.Subject
    $removed = 0

    foreach ($storeName in @("Root", "TrustedPublisher")) {
        $storePath = "Cert:\LocalMachine\$storeName"
        foreach ($certificate in @(Get-ChildItem -LiteralPath $storePath -ErrorAction Stop)) {
            if ($certificate.Subject -eq $expectedSubject -and
                $certificate.Thumbprint.ToUpperInvariant() -ne $expectedThumbprint) {
                Write-CleanupLog "remove_certificate store=$storeName thumbprint=$($certificate.Thumbprint) subject=$($certificate.Subject)"
                Remove-Item -LiteralPath $certificate.PSPath -Force -ErrorAction Stop
                $removed++
            }
        }
    }

    $remainingOld = @()
    foreach ($storeName in @("Root", "TrustedPublisher")) {
        $remainingOld += @(Get-ChildItem -LiteralPath "Cert:\LocalMachine\$storeName" -ErrorAction Stop |
            Where-Object {
                $_.Subject -eq $expectedSubject -and
                $_.Thumbprint.ToUpperInvariant() -ne $expectedThumbprint
            })
    }
    if ($remainingOld.Count -ne 0) {
        throw "Old BridgeVM viogpu3d certificates remain after cleanup: $($remainingOld.Count)"
    }

    Write-CleanupLog "certificates_clean removed=$removed preserved_thumbprint=$expectedThumbprint subject=$expectedSubject"
}

function Remove-VioGpu3DDriverPackages {
    $publishedInfs = @(Get-VioGpu3DPublishedInfs)
    Write-CleanupLog "driverstore_cleanup_begin published_inf_count=$($publishedInfs.Count)"

    foreach ($inf in $publishedInfs) {
        Write-CleanupLog "delete_driver_begin published_inf=$($inf.Name)"
        $output = @(& pnputil.exe /delete-driver $inf.Name /uninstall /force 2>&1)
        $status = $LASTEXITCODE
        foreach ($line in $output) {
            Write-CleanupLog ("pnputil published_inf={0} output={1}" -f $inf.Name, ([string]$line).Trim())
        }
        if ($status -ne 0 -and $status -ne 3010) {
            throw "pnputil failed to delete $($inf.Name): exit=$status"
        }
        Write-CleanupLog "delete_driver_done published_inf=$($inf.Name) exit=$status"
    }

    $remaining = @(Get-VioGpu3DPublishedInfs)
    if ($remaining.Count -ne 0) {
        throw "Published viogpu3d INF packages remain before cleanup reboot: $($remaining.Name -join ',')"
    }
    Write-CleanupLog "driverstore_cleanup_complete removed=$($publishedInfs.Count)"
}

function Assert-CleanDriverStore {
    $publishedInfs = @(Get-VioGpu3DPublishedInfs)
    $repositoryDirectories = @(Get-VioGpu3DRepositoryDirectories)
    Write-CleanupLog "verify_clean published_inf_count=$($publishedInfs.Count) repository_dir_count=$($repositoryDirectories.Count)"
    if ($publishedInfs.Count -ne 0) {
        throw "Published viogpu3d INF packages survived the cleanup reboot: $($publishedInfs.Name -join ',')"
    }
    if ($repositoryDirectories.Count -ne 0) {
        throw "viogpu3d FileRepository directories survived the cleanup reboot: $($repositoryDirectories.Name -join ',')"
    }
    Write-CleanupLog "BVGPU-DRIVERSTORE-CLEAN-PASS"
}

function Assert-InstalledDriverState {
    if (-not (Test-Path -LiteralPath $expectedInf -PathType Leaf)) {
        throw "Expected staged INF is missing: $expectedInf"
    }
    $expectedHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $expectedInf).Hash
    $publishedInfs = @(Get-VioGpu3DPublishedInfs)
    $repositoryDirectories = @(Get-VioGpu3DRepositoryDirectories)
    if ($publishedInfs.Count -ne 1) {
        throw "Expected exactly one published viogpu3d INF, found $($publishedInfs.Count): $($publishedInfs.Name -join ',')"
    }
    if ($repositoryDirectories.Count -ne 1) {
        throw "Expected exactly one viogpu3d FileRepository directory, found $($repositoryDirectories.Count): $($repositoryDirectories.Name -join ',')"
    }
    $publishedHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $publishedInfs[0].FullName).Hash
    if ($publishedHash -ne $expectedHash) {
        throw "The sole published viogpu3d INF does not match the injected package: published=$publishedHash expected=$expectedHash"
    }

    $expected = Get-ExpectedCertificate
    $expectedThumbprint = $expected.Thumbprint.ToUpperInvariant()
    foreach ($storeName in @("Root", "TrustedPublisher")) {
        $matching = @(Get-ChildItem -LiteralPath "Cert:\LocalMachine\$storeName" -ErrorAction Stop |
            Where-Object { $_.Subject -eq $expected.Subject })
        if ($matching.Count -ne 1 -or $matching[0].Thumbprint.ToUpperInvariant() -ne $expectedThumbprint) {
            throw "Certificate store $storeName is not clean for subject $($expected.Subject): count=$($matching.Count)"
        }
    }

    Write-CleanupLog "verify_installed published_inf=$($publishedInfs[0].Name) inf_sha256=$expectedHash repository_dir=$($repositoryDirectories[0].Name) certificate_thumbprint=$expectedThumbprint"
    Write-CleanupLog "BVGPU-DRIVER-STATE-PASS"
}

Write-CleanupLog "begin"
switch ($Phase) {
    "Certificates" { Remove-OldCertificates }
    "DriverStore" { Remove-VioGpu3DDriverPackages }
    "VerifyClean" { Assert-CleanDriverStore }
    "VerifyInstalled" { Assert-InstalledDriverState }
}
Write-CleanupLog "done"
