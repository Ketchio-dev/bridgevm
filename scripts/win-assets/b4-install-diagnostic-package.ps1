[CmdletBinding()]
param([switch]$VerifyOnly)

$ErrorActionPreference = 'Stop'
$Share = 'C:\BridgeVMPtr'
$ManifestPath = Join-Path $Share 'b4-package-manifest.tsv'
$PackageRoot = 'C:\BridgeVM\B4DiagPackage'
$ResultPath = Join-Path $Share $(if ($VerifyOnly) { 'b4-verify-result.log' } else { 'b4-install-result.log' })
$ExpectedVersion = '120.50.0.0'
$Names = @(
  'BridgeVM-viogpu3d-Test.cer',
  'bridgevm-package-provenance.env',
  'viogpu3d.cat',
  'viogpu3d.inf',
  'viogpu3d.sys',
  'viogpu_d3d10.dll',
  'virtio_icd.arm64.json',
  'vulkan_virtio.dll'
)

function Fail([string]$Message) { throw $Message }
function Hash([string]$Path) {
  (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
}
function Write-Result([string[]]$Lines) {
  Set-Content -LiteralPath $ResultPath -Value $Lines -Encoding Ascii
}

try {
  Remove-Item -LiteralPath $ResultPath -Force -ErrorAction SilentlyContinue
  $lines = @(Get-Content -LiteralPath $ManifestPath -Encoding Ascii)
  if ($lines.Count -lt 18 -or $lines[0] -cne "format`tbridgevm-b4-package-v1") {
    Fail 'invalid package manifest framing'
  }
  $packageFields = @($lines[1] -split "`t")
  if ($packageFields.Count -ne 2 -or $packageFields[0] -cne 'package_sha256' -or
      $packageFields[1] -cnotmatch '^[0-9a-f]{64}$') { Fail 'invalid package tree hash' }
  $files = @{}
  $chunks = @{}
  foreach ($line in $lines[2..($lines.Count - 1)]) {
    $fields = @($line -split "`t")
    if ($fields[0] -ceq 'file') {
      if ($fields.Count -ne 6 -or $fields[1] -cnotmatch '^[0-7]$') { Fail 'invalid file row' }
      $index = [int]$fields[1]
      if ($fields[2] -cne $Names[$index] -or $fields[3] -cnotmatch '^[1-9][0-9]*$' -or
          $fields[4] -cnotmatch '^[0-9a-f]{64}$' -or $fields[5] -cnotmatch '^[1-9][0-9]*$' -or
          $files.ContainsKey($index)) { Fail 'invalid or duplicate file row' }
      $files[$index] = [pscustomobject]@{ Name=$fields[2]; Size=[long]$fields[3]
        Hash=$fields[4]; Count=[int]$fields[5] }
      if ($files[$index].Size -gt 67108864 -or $files[$index].Count -gt 64) {
        Fail ('file bounds exceeded: ' + $fields[2])
      }
      $chunks[$index] = New-Object Collections.ArrayList
    } elseif ($fields[0] -ceq 'chunk') {
      if ($fields.Count -ne 6 -or $fields[1] -cnotmatch '^[0-7]$' -or
          $fields[2] -cnotmatch '^[0-9]+$' -or
          $fields[3] -cnotmatch '^b4pkg-[0-7][0-9]-[0-9]{3}[.]bin$' -or
          $fields[4] -cnotmatch '^[1-9][0-9]*$' -or $fields[5] -cnotmatch '^[0-9a-f]{64}$') {
        Fail 'invalid chunk row'
      }
      $index = [int]$fields[1]
      if (-not $chunks.ContainsKey($index)) { Fail 'chunk precedes file row' }
      if ([long]$fields[4] -gt 1048576) { Fail 'chunk exceeds 1 MiB' }
      [void]$chunks[$index].Add([pscustomobject]@{ Index=[int]$fields[2]
        Name=$fields[3]; Size=[long]$fields[4]; Hash=$fields[5] })
    } else { Fail 'unknown package manifest row' }
  }
  if ($files.Count -ne $Names.Count) { Fail 'package manifest file count mismatch' }
  $treeText = New-Object Text.StringBuilder
  for ($index = 0; $index -lt $Names.Count; $index++) {
    [void]$treeText.Append($Names[$index]).Append([char]0).Append($files[$index].Hash).Append("`n")
  }
  $sha = [Security.Cryptography.SHA256]::Create()
  try { $computedTree = ([BitConverter]::ToString($sha.ComputeHash(
      [Text.Encoding]::ASCII.GetBytes($treeText.ToString())))).Replace('-', '').ToLowerInvariant() }
  finally { $sha.Dispose() }
  if ($computedTree -cne $packageFields[1]) { Fail 'package tree hash mismatch' }

  if (-not $VerifyOnly) {
    Remove-Item -LiteralPath $PackageRoot -Recurse -Force -ErrorAction SilentlyContinue
    New-Item -ItemType Directory -Path $PackageRoot | Out-Null
    for ($index = 0; $index -lt $Names.Count; $index++) {
      $file = $files[$index]
      $parts = @($chunks[$index] | Sort-Object Index)
      if ($parts.Count -ne $file.Count) { Fail ('chunk count mismatch: ' + $file.Name) }
      $destination = Join-Path $PackageRoot $file.Name
      $output = [IO.File]::Open($destination, [IO.FileMode]::CreateNew,
        [IO.FileAccess]::Write, [IO.FileShare]::None)
      try {
        for ($partIndex = 0; $partIndex -lt $parts.Count; $partIndex++) {
          $part = $parts[$partIndex]
          if ($part.Index -ne $partIndex) { Fail ('chunk index mismatch: ' + $file.Name) }
          $expectedChunkName = 'b4pkg-{0:d2}-{1:d3}.bin' -f $index, $partIndex
          if ($part.Name -cne $expectedChunkName) { Fail ('chunk name mismatch: ' + $file.Name) }
          $path = Join-Path $Share $part.Name
          $item = Get-Item -LiteralPath $path
          if ($item.Length -ne $part.Size -or (Hash $path) -cne $part.Hash) {
            Fail ('chunk verification failed: ' + $part.Name)
          }
          $input = [IO.File]::OpenRead($path)
          try { $input.CopyTo($output) } finally { $input.Dispose() }
        }
      } finally { $output.Dispose() }
      $item = Get-Item -LiteralPath $destination
      if ($item.Length -ne $file.Size -or (Hash $destination) -cne $file.Hash) {
        Fail ('reassembled file verification failed: ' + $file.Name)
      }
    }
    $certificate = Join-Path $PackageRoot 'BridgeVM-viogpu3d-Test.cer'
    & certutil.exe -addstore -f Root $certificate | Out-File -LiteralPath $ResultPath -Append -Encoding Ascii
    if ($LASTEXITCODE -ne 0) { Fail 'Root certificate import failed' }
    & certutil.exe -addstore -f TrustedPublisher $certificate | Out-File -LiteralPath $ResultPath -Append -Encoding Ascii
    if ($LASTEXITCODE -ne 0) { Fail 'TrustedPublisher certificate import failed' }
    & pnputil.exe /add-driver (Join-Path $PackageRoot 'viogpu3d.inf') /install |
      Out-File -LiteralPath $ResultPath -Append -Encoding Ascii
    $pnputil = $LASTEXITCODE
    if ($pnputil -ne 0 -and $pnputil -ne 3010) { Fail ('pnputil failed: ' + $pnputil) }
    Write-Result @('install_ready=true', ('package_sha256=' + $packageFields[1]),
      ('pnputil_exit=' + $pnputil), ('expected_version=' + $ExpectedVersion))
    exit 0
  }

  for ($index = 0; $index -lt $Names.Count; $index++) {
    $path = Join-Path $PackageRoot $Names[$index]
    $item = Get-Item -LiteralPath $path
    if ($item.Length -ne $files[$index].Size -or (Hash $path) -cne $files[$index].Hash) {
      Fail ('retained package verification failed: ' + $Names[$index])
    }
  }
  $device = Get-PnpDevice -PresentOnly -ErrorAction Stop | Where-Object {
    $_.InstanceId -match '^PCI\\VEN_1AF4&DEV_(1050|10F7)(?:&|$)' -and $_.Status -eq 'OK'
  } | Select-Object -First 1
  if (-not $device) { Fail 'bound virtio-gpu device is not present and OK' }
  $driver = Get-CimInstance Win32_PnPSignedDriver | Where-Object {
    $_.DeviceID -eq $device.InstanceId
  } | Select-Object -First 1
  if (-not $driver -or $driver.DriverVersion -cne $ExpectedVersion -or
      $driver.InfName -cnotmatch '^oem[0-9]+[.]inf$') { Fail '120.50 driver is not bound' }
  $boundInf = Join-Path $env:windir ('INF\' + $driver.InfName)
  $infHash = Hash (Join-Path $PackageRoot 'viogpu3d.inf')
  if ((Hash $boundInf) -cne $infHash) { Fail 'bound OEM INF differs from diagnostic package' }
  $repository = Join-Path $env:windir 'System32\DriverStore\FileRepository'
  $umdHash = $files[5].Hash
  $umdMatches = @(Get-ChildItem -LiteralPath $repository -Filter 'viogpu_d3d10.dll' -File -Recurse |
    Where-Object { (Hash $_.FullName) -ceq $umdHash })
  if ($umdMatches.Count -lt 1) { Fail 'diagnostic UMD is absent from DriverStore' }
  Write-Result @('verified=true', ('package_sha256=' + $packageFields[1]),
    ('driver_version=' + $driver.DriverVersion), ('bound_inf_sha256=' + $infHash),
    ('installed_umd_sha256=' + $umdHash), ('installed_umd_matches=' + $umdMatches.Count))
} catch {
  Write-Result @('verified=false', ('error_type=' + $_.Exception.GetType().FullName),
    ('error=' + $_.Exception.Message.Replace("`r", ' ').Replace("`n", ' ')))
  exit 1
}
