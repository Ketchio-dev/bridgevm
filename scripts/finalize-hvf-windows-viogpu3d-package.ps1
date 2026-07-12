[CmdletBinding()]
param(
    [string]$PackageDir = (Join-Path $PSScriptRoot "package"),
    [string]$FinalizedDir = "",
    [Parameter(Mandatory = $true)]
    [string]$PreFinalizationManifest,
    [Parameter(Mandatory = $true)]
    [string]$CertificatePfx,
    [string]$CertificatePassword = "",
    [string]$Inf2CatOs = "auto",
    [string]$TimestampUrl = "",
    [switch]$TestSigning,
    [ValidateSet("auto", "arehnman-arm64-minimal")]
    [string]$Profile = "auto"
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Resolve-RequiredTool {
    param([Parameter(Mandatory = $true)][string]$Name)
    $command = Get-Command $Name -ErrorAction SilentlyContinue
    if ($null -ne $command) {
        return $command.Source
    }

    # Winget Windows SDK/WDK installs do not update the environment of an
    # already-running BVAGENT process. Discover versioned Windows Kits tools so
    # finalization works in that same boot without restarting the guest. The WDK
    # supplies InfVerif/Inf2Cat while the Windows SDK supplies SignTool.
    $kitRoots = @()
    foreach ($programFilesRoot in @(${env:ProgramFiles(x86)}, $env:ProgramFiles)) {
        if ([string]::IsNullOrWhiteSpace($programFilesRoot)) {
            continue
        }
        foreach ($relativeRoot in @("Windows Kits\10\bin", "Windows Kits\10\Tools")) {
            $kitRoot = Join-Path $programFilesRoot $relativeRoot
            if (Test-Path -LiteralPath $kitRoot -PathType Container) {
                $kitRoots += $kitRoot
            }
        }
    }
    $kitRoots = @($kitRoots | Select-Object -Unique)
    $candidates = @()
    foreach ($root in $kitRoots) {
        $candidates += @(Get-ChildItem -LiteralPath $root -Filter $Name -File -Recurse -ErrorAction SilentlyContinue)
    }
    $nativeArchitecture = if ($env:PROCESSOR_ARCHITEW6432) {
        $env:PROCESSOR_ARCHITEW6432
    } else {
        $env:PROCESSOR_ARCHITECTURE
    }
    $architectureRank = @{ arm64 = 2; x64 = 1; x86 = 0 }
    if ($nativeArchitecture -eq "ARM64") {
        $architectureRank = @{ arm64 = 0; x64 = 1; x86 = 2 }
    } elseif ($nativeArchitecture -eq "AMD64") {
        $architectureRank = @{ x64 = 0; x86 = 1; arm64 = 2 }
    }
    $candidate = $candidates |
        Sort-Object @{ Expression = {
            $leaf = $_.Directory.Name.ToLowerInvariant()
            if ($architectureRank.ContainsKey($leaf)) { $architectureRank[$leaf] } else { 3 }
        } }, @{ Expression = { $_.FullName }; Descending = $true } |
        Select-Object -First 1
    if ($null -eq $candidate) {
        throw "Required Windows SDK/WDK tool was not found in PATH or Windows Kits bin/Tools: $Name"
    }
    return $candidate.FullName
}

function Invoke-ExternalTool {
    param(
        [Parameter(Mandatory = $true)][string]$Tool,
        [Parameter(Mandatory = $true)][string[]]$Arguments,
        [Parameter(Mandatory = $true)][string]$Label
    )
    # The disposable wrapper hands the PFX password to this PowerShell process
    # through its environment. Do not let unrelated SDK/WDK child processes
    # inherit that secret. Machine-store signing needs no password argument;
    # the standalone PFX compatibility path still supplies /p explicitly.
    $passwordEnvironment = [Environment]::GetEnvironmentVariable(
        "VIOGPU3D_CERTIFICATE_PASSWORD",
        [EnvironmentVariableTarget]::Process
    )
    try {
        [Environment]::SetEnvironmentVariable(
            "VIOGPU3D_CERTIFICATE_PASSWORD",
            $null,
            [EnvironmentVariableTarget]::Process
        )
        & $Tool @Arguments
        $exitCode = $LASTEXITCODE
    } finally {
        [Environment]::SetEnvironmentVariable(
            "VIOGPU3D_CERTIFICATE_PASSWORD",
            $passwordEnvironment,
            [EnvironmentVariableTarget]::Process
        )
    }
    if ($exitCode -ne 0) {
        throw "$Label failed with exit code $exitCode"
    }
}

function Invoke-ExternalToolCapture {
    param(
        [Parameter(Mandatory = $true)][string]$Tool,
        [Parameter(Mandatory = $true)][string[]]$Arguments,
        [Parameter(Mandatory = $true)][string]$Label
    )

    $passwordEnvironment = [Environment]::GetEnvironmentVariable(
        "VIOGPU3D_CERTIFICATE_PASSWORD",
        [EnvironmentVariableTarget]::Process
    )
    $previousErrorActionPreference = $ErrorActionPreference
    $output = ""
    $exitCode = -1
    try {
        [Environment]::SetEnvironmentVariable(
            "VIOGPU3D_CERTIFICATE_PASSWORD",
            $null,
            [EnvironmentVariableTarget]::Process
        )
        # Windows PowerShell 5.1 turns native stderr into error records. Keep
        # the complete help/error stream as text and judge the native exit code.
        $ErrorActionPreference = "Continue"
        $output = (& $Tool @Arguments 2>&1 | Out-String)
        $exitCode = $LASTEXITCODE
    } finally {
        $ErrorActionPreference = $previousErrorActionPreference
        [Environment]::SetEnvironmentVariable(
            "VIOGPU3D_CERTIFICATE_PASSWORD",
            $passwordEnvironment,
            [EnvironmentVariableTarget]::Process
        )
    }
    if ($exitCode -ne 0) {
        throw "$Label failed with exit code $exitCode`n$output"
    }
    return $output
}

function Resolve-Inf2CatOperatingSystem {
    param(
        [Parameter(Mandatory = $true)][string]$Tool,
        [Parameter(Mandatory = $true)][string]$Requested
    )

    if ($Requested -ne "auto") {
        return $Requested
    }

    # ARM64 did not have the unversioned 10_ARM64 token used by x86/x64.
    # Select the newest token advertised by the installed WDK, retaining
    # compatibility with older kits while preferring current Windows 11 GE.
    $help = Invoke-ExternalToolCapture -Tool $Tool -Arguments @("/?") -Label "Inf2Cat help"
    $candidates = @(
        "10_GE_ARM64",
        "10_NI_ARM64",
        "10_CO_ARM64",
        "10_VB_ARM64",
        "10_19H1_ARM64",
        "10_RS5_ARM64",
        "10_RS4_ARM64",
        "10_RS3_ARM64"
    )
    foreach ($candidate in $candidates) {
        $pattern = "(?m)(^|\s)" + [Regex]::Escape($candidate) + "(\s|$)"
        if ([Regex]::IsMatch($help, $pattern)) {
            return $candidate
        }
    }
    throw "Installed Inf2Cat does not advertise a supported Windows ARM64 OS token"
}

function Assert-Arm64Pe {
    param([Parameter(Mandatory = $true)][string]$Path)
    $bytes = [System.IO.File]::ReadAllBytes($Path)
    if ($bytes.Length -lt 68 -or $bytes[0] -ne 0x4d -or $bytes[1] -ne 0x5a) {
        throw "Not a PE/MZ image: $Path"
    }
    $peOffset = [System.BitConverter]::ToInt32($bytes, 0x3c)
    if ($peOffset -lt 0 -or $peOffset + 6 -gt $bytes.Length) {
        throw "Invalid PE header offset: $Path"
    }
    if ($bytes[$peOffset] -ne 0x50 -or $bytes[$peOffset + 1] -ne 0x45 -or
        $bytes[$peOffset + 2] -ne 0 -or $bytes[$peOffset + 3] -ne 0) {
        throw "Invalid PE signature: $Path"
    }
    $machine = [System.BitConverter]::ToUInt16($bytes, $peOffset + 4)
    if ($machine -ne 0xaa64) {
        throw ("PE image is not ARM64 (machine=0x{0:x4}): {1}" -f $machine, $Path)
    }
}

function Assert-PinnedInputManifest {
    param(
        [Parameter(Mandatory = $true)][string]$ManifestPath,
        [Parameter(Mandatory = $true)][string]$InputDir,
        [Parameter(Mandatory = $true)][string[]]$ExpectedNames
    )

    $entries = @{}
    foreach ($line in @(Get-Content -LiteralPath $ManifestPath)) {
        if ([string]::IsNullOrWhiteSpace($line)) {
            continue
        }
        if ($line -notmatch '^([0-9A-Fa-f]{64}) [ *]([^\\/:\r\n]+)$') {
            throw "Invalid pre-finalization manifest line: $line"
        }
        $hash = $Matches[1].ToLowerInvariant()
        $name = $Matches[2]
        if (-not ($ExpectedNames -ccontains $name)) {
            throw "Unexpected file in pre-finalization manifest: $name"
        }
        if ($entries.ContainsKey($name)) {
            throw "Duplicate file in pre-finalization manifest: $name"
        }
        $entries.Add($name, $hash)
    }

    $expectedSorted = @($ExpectedNames | Sort-Object)
    $actualSorted = @($entries.Keys | Sort-Object)
    $difference = @(Compare-Object -ReferenceObject $expectedSorted -DifferenceObject $actualSorted -CaseSensitive)
    if ($difference.Count -ne 0) {
        throw "Pre-finalization manifest does not contain the exact pinned input set"
    }

    foreach ($name in $ExpectedNames) {
        $path = Join-Path $InputDir $name
        if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
            throw "Pinned input file is missing: $path"
        }
        $actualHash = (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant()
        if ($actualHash -cne $entries[$name]) {
            throw "Pre-finalization SHA-256 mismatch for $name"
        }
    }
}

function Sign-Artifact {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][string]$SignTool,
        [Parameter(Mandatory = $true)][bool]$UseMachineStore,
        [Parameter(Mandatory = $true)][string]$CertificateThumbprint
    )
    if ($UseMachineStore) {
        # The disposable wrapper creates its identity in LocalMachine\My. Select
        # it by thumbprint so the random PFX password never enters a child
        # process command line. A standalone PFX remains the compatibility path.
        $arguments = @("sign", "/fd", "SHA256", "/sm", "/sha1", $CertificateThumbprint)
    } else {
        $arguments = @("sign", "/fd", "SHA256", "/f", $CertificatePfx)
        if ($CertificatePassword -ne "") {
            $arguments += @("/p", $CertificatePassword)
        }
    }
    if ($TimestampUrl -ne "") {
        $arguments += @("/tr", $TimestampUrl, "/td", "SHA256")
    }
    $arguments += $Path
    Invoke-ExternalTool -Tool $SignTool -Arguments $arguments -Label "SignTool sign $Path"
}

if ($CertificatePassword -eq "" -and $env:VIOGPU3D_CERTIFICATE_PASSWORD) {
    $CertificatePassword = $env:VIOGPU3D_CERTIFICATE_PASSWORD
}

$PackageDir = [System.IO.Path]::GetFullPath($PackageDir)
$PreFinalizationManifest = [System.IO.Path]::GetFullPath($PreFinalizationManifest)
$CertificatePfx = [System.IO.Path]::GetFullPath($CertificatePfx)
if ($FinalizedDir -eq "") {
    $FinalizedDir = "$PackageDir-finalized"
}
$FinalizedDir = [System.IO.Path]::GetFullPath($FinalizedDir)

if (-not (Test-Path -LiteralPath $PackageDir -PathType Container)) {
    throw "Package directory does not exist: $PackageDir"
}
if (-not (Test-Path -LiteralPath $PreFinalizationManifest -PathType Leaf)) {
    throw "Pre-finalization manifest does not exist: $PreFinalizationManifest"
}
if (-not (Test-Path -LiteralPath $CertificatePfx -PathType Leaf)) {
    throw "Certificate PFX does not exist: $CertificatePfx"
}
if (Test-Path -LiteralPath $FinalizedDir) {
    throw "Finalized output already exists; restage or choose a new -FinalizedDir: $FinalizedDir"
}
$packagePrefix = $PackageDir + [System.IO.Path]::DirectorySeparatorChar
if ($FinalizedDir.StartsWith($packagePrefix, [System.StringComparison]::OrdinalIgnoreCase)) {
    throw "FinalizedDir must not be inside the unsigned input package: $FinalizedDir"
}

$InfVerif = Resolve-RequiredTool "InfVerif.exe"
$Inf2Cat = Resolve-RequiredTool "Inf2Cat.exe"
$SignTool = Resolve-RequiredTool "signtool.exe"
$Inf2CatOs = Resolve-Inf2CatOperatingSystem -Tool $Inf2Cat -Requested $Inf2CatOs

$coreDllNames = @(
    "libEGL_arm64.dll",
    "libGLESv2_arm64.dll",
    "opengl32_arm64.dll",
    "viogpu_d3d10_arm64.dll",
    "viogpu_wgl_arm64.dll"
)
$expectedInputNames = @(
    "bridgevm-package-provenance.env",
    "viogpu3d.inf",
    "viogpu3d.sys"
) + $coreDllNames

$inputItems = @(Get-ChildItem -LiteralPath $PackageDir -Force -Recurse)
foreach ($item in $inputItems) {
    if (($item.Attributes -band [System.IO.FileAttributes]::ReparsePoint) -ne 0) {
        throw "Input package must not contain links or reparse points: $($item.FullName)"
    }
    if ($item.PSIsContainer) {
        throw "Minimal input package must be flat and contain no directories: $($item.FullName)"
    }
    if (-not [string]::Equals($item.DirectoryName, $PackageDir, [System.StringComparison]::OrdinalIgnoreCase)) {
        throw "Minimal input package file is not in the package root: $($item.FullName)"
    }
}
$actualInputNames = @($inputItems | ForEach-Object { $_.Name } | Sort-Object)
$expectedInputNamesSorted = @($expectedInputNames | Sort-Object)
$inputDifference = @(Compare-Object -ReferenceObject $expectedInputNamesSorted -DifferenceObject $actualInputNames -CaseSensitive)
if ($inputDifference.Count -ne 0) {
    throw "Unsigned input package does not contain the exact pinned minimal-profile file set"
}

$InputInf = Join-Path $PackageDir "viogpu3d.inf"
$InputSys = Join-Path $PackageDir "viogpu3d.sys"
$InputProvenance = Join-Path $PackageDir "bridgevm-package-provenance.env"
$provenanceText = Get-Content -LiteralPath $InputProvenance -Raw
if ($Profile -eq "auto") {
    if ($provenanceText -match '(?m)^VIOGPU3D_BUILD_ID=arehnman-arm64-minimal-') {
        $Profile = "arehnman-arm64-minimal"
    } else {
        throw "Cannot infer the pinned minimal package profile from provenance; pass -Profile explicitly"
    }
}

Assert-PinnedInputManifest `
    -ManifestPath $PreFinalizationManifest `
    -InputDir $PackageDir `
    -ExpectedNames $expectedInputNames

$expectedInfSha256 = "f8bc2e3bb097d1d8f9d461745dc6665b65bddf53cbb986dc57df1059f374b5e9"
$actualInfSha256 = (Get-FileHash -LiteralPath $InputInf -Algorithm SHA256).Hash.ToLowerInvariant()
if ($actualInfSha256 -cne $expectedInfSha256) {
    throw "viogpu3d.inf is not the pinned BridgeVM arehnman-arm64-minimal template"
}

$inputDllFiles = @(Get-ChildItem -LiteralPath $PackageDir -File -Filter "*.dll")
$actualDllNames = @($inputDllFiles | ForEach-Object { $_.Name } | Sort-Object)
$dllDifference = @(Compare-Object -ReferenceObject $coreDllNames -DifferenceObject $actualDllNames -CaseSensitive)
if ($dllDifference.Count -ne 0) {
    throw "Package DLL set does not match the pinned $Profile profile"
}

$requiredInfText = @(
    "PCI\VEN_1AF4&DEV_1050",
    "VioGpu3D_Files.Usermode=11",
    "UserModeDriverName",
    "%11%\viogpu_d3d10.dll",
    "OpenGLDriverName",
    "%11%\viogpu_wgl.dll",
    "OpenGLFlags",
    "OpenGLVersion",
    "InstalledDisplayDrivers",
    "CopyFiles=VioGpu3D_Files.Driver,VioGpu3D_Files.Usermode"
)
foreach ($needle in $requiredInfText) {
    if (-not (Select-String -LiteralPath $InputInf -SimpleMatch $needle -Quiet)) {
        throw "INF is missing the required render-package contract: $needle"
    }
}

Assert-Arm64Pe $InputSys
foreach ($dll in $inputDllFiles) {
    Assert-Arm64Pe $dll.FullName
}

$certificateFlags = [System.Security.Cryptography.X509Certificates.X509KeyStorageFlags]::DefaultKeySet
$certificate = [System.Security.Cryptography.X509Certificates.X509Certificate2]::new(
    $CertificatePfx,
    $CertificatePassword,
    $certificateFlags
)
if (-not $certificate.HasPrivateKey) {
    $certificate.Dispose()
    throw "Certificate PFX has no private key: $CertificatePfx"
}
$hasCodeSigningEku = $false
foreach ($extension in $certificate.Extensions) {
    if ($extension -is [System.Security.Cryptography.X509Certificates.X509EnhancedKeyUsageExtension]) {
        foreach ($usage in $extension.EnhancedKeyUsages) {
            if ($usage.Value -eq "1.3.6.1.5.5.7.3.3") {
                $hasCodeSigningEku = $true
            }
        }
    }
}
if (-not $hasCodeSigningEku) {
    $certificate.Dispose()
    throw "Certificate PFX does not advertise the Code Signing EKU"
}
$machineStoreCertificate = Get-Item -LiteralPath ("Cert:\LocalMachine\My\" + $certificate.Thumbprint) -ErrorAction SilentlyContinue
$signFromMachineStore = $null -ne $machineStoreCertificate -and $machineStoreCertificate.HasPrivateKey
$signingSource = if ($signFromMachineStore) { "local-machine-store" } else { "pfx-file" }

$finalizedParent = Split-Path -Parent $FinalizedDir
New-Item -ItemType Directory -Force -Path $finalizedParent | Out-Null
$finalizedLeaf = Split-Path -Leaf $FinalizedDir
$workingPackageDir = Join-Path $finalizedParent (".$finalizedLeaf.finalizing." + [Guid]::NewGuid().ToString("N"))

try {
    New-Item -ItemType Directory -Path $workingPackageDir | Out-Null
    foreach ($name in $expectedInputNames) {
        Copy-Item -LiteralPath (Join-Path $PackageDir $name) -Destination (Join-Path $workingPackageDir $name)
    }

    $Inf = Join-Path $workingPackageDir "viogpu3d.inf"
    $Sys = Join-Path $workingPackageDir "viogpu3d.sys"
    $Cat = Join-Path $workingPackageDir "viogpu3d.cat"
    $Provenance = Join-Path $workingPackageDir "bridgevm-package-provenance.env"
    $Cer = Join-Path $workingPackageDir "BridgeVM-viogpu3d-Test.cer"
    $Report = Join-Path $workingPackageDir "bridgevm-finalization-report.txt"
    $dllFiles = @($coreDllNames | ForEach-Object { Get-Item -LiteralPath (Join-Path $workingPackageDir $_) })

    [System.IO.File]::WriteAllBytes(
        $Cer,
        $certificate.Export([System.Security.Cryptography.X509Certificates.X509ContentType]::Cert)
    )

    $provenanceLines = @(
        Get-Content -LiteralPath $Provenance |
            Where-Object { $_ -notmatch '^VIOGPU3D_SIGNING_CERT=' }
    )
    $provenanceLines += "VIOGPU3D_SIGNING_CERT=test-signing thumbprint=$($certificate.Thumbprint) subject=$($certificate.Subject)"
    $provenanceLines | Set-Content -LiteralPath $Provenance -Encoding Ascii

    Invoke-ExternalTool -Tool $InfVerif -Arguments @("/v", $Inf) -Label "InfVerif"
    Sign-Artifact -Path $Sys -SignTool $SignTool -UseMachineStore $signFromMachineStore -CertificateThumbprint $certificate.Thumbprint
    foreach ($dll in $dllFiles) {
        Sign-Artifact -Path $dll.FullName -SignTool $SignTool -UseMachineStore $signFromMachineStore -CertificateThumbprint $certificate.Thumbprint
    }
    Invoke-ExternalTool -Tool $Inf2Cat -Arguments @(
        "/driver:$workingPackageDir",
        "/os:$Inf2CatOs",
        "/uselocaltime"
    ) -Label "Inf2Cat"
    if (-not (Test-Path -LiteralPath $Cat -PathType Leaf)) {
        throw "Inf2Cat completed without creating the expected catalog: $Cat"
    }
    Sign-Artifact -Path $Cat -SignTool $SignTool -UseMachineStore $signFromMachineStore -CertificateThumbprint $certificate.Thumbprint

    Invoke-ExternalTool -Tool $SignTool -Arguments @("verify", "/v", "/pa", $Sys) -Label "SignTool Authenticode verify SYS"
    if (-not $TestSigning) {
        Invoke-ExternalTool -Tool $SignTool -Arguments @("verify", "/v", "/kp", $Sys) -Label "SignTool kernel-policy verify SYS"
    }
    foreach ($dll in $dllFiles) {
        Invoke-ExternalTool -Tool $SignTool -Arguments @("verify", "/v", "/pa", $dll.FullName) -Label "SignTool Authenticode verify $($dll.Name)"
    }
    Invoke-ExternalTool -Tool $SignTool -Arguments @("verify", "/v", "/pa", $Cat) -Label "SignTool Authenticode verify CAT"
    if (-not $TestSigning) {
        Invoke-ExternalTool -Tool $SignTool -Arguments @("verify", "/v", "/kp", $Cat) -Label "SignTool kernel-policy verify CAT"
    }

    $artifactPaths = @($Inf, $Sys, $Cat, $Cer, $Provenance) + @($dllFiles | ForEach-Object { $_.FullName })
    $signingMode = if ($TestSigning) { "test" } else { "kernel-policy" }
    $testSigningRequired = if ($TestSigning) { "true" } else { "false" }
    $kernelPolicyVerified = if ($TestSigning) { "false" } else { "true" }
    $kernelPolicyStatus = if ($TestSigning) { "skipped-self-signed-test-root" } else { "passed" }
    $reportLines = @(
        "BridgeVM viogpu3d Windows WDK finalization",
        "finalization_complete=true",
        "profile=$Profile",
        "package_dir=$FinalizedDir",
        "input_package_dir=$PackageDir",
        "pre_finalization_manifest=$PreFinalizationManifest",
        "infverif=$InfVerif",
        "inf2cat=$Inf2Cat",
        "signtool=$SignTool",
        "inf2cat_os=$Inf2CatOs",
        "signing_cert_thumbprint=$($certificate.Thumbprint)",
        "signing_cert_subject=$($certificate.Subject)",
        "signing_source=$signingSource",
        "signing_mode=$signingMode",
        "test_signing_required=$testSigningRequired",
        "sys_authenticode_verified=true",
        "sys_kernel_policy_verified=$kernelPolicyVerified",
        "dll_authenticode_verified=true",
        "cat_authenticode_verified=true",
        "cat_kernel_policy_verified=$kernelPolicyVerified",
        "kernel_policy_status=$kernelPolicyStatus",
        "bridgevm_render_candidate_check_required=true"
    )
    foreach ($path in $artifactPaths) {
        if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
            throw "Final artifact is missing before report generation: $path"
        }
        $hash = (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant()
        $name = [System.IO.Path]::GetFileName($path)
        $reportLines += "sha256.$name=$hash"
    }
    $reportLines | Set-Content -LiteralPath $Report -Encoding Ascii

    [System.IO.Directory]::Move($workingPackageDir, $FinalizedDir)
    $workingPackageDir = $null
} finally {
    $certificate.Dispose()
    if ($null -ne $workingPackageDir -and (Test-Path -LiteralPath $workingPackageDir)) {
        Remove-Item -LiteralPath $workingPackageDir -Recurse -Force
    }
}

Write-Host "BridgeVM viogpu3d package finalized: $FinalizedDir"
Write-Host "Finalization report: $(Join-Path $FinalizedDir 'bridgevm-finalization-report.txt')"
Write-Host "Required next gate on the Mac: check-hvf-windows-viogpu3d-package.sh --require-render-candidate"
