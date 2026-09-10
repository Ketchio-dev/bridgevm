# Diagnostic reset inside a disposable B6 guest; failure must block the scene.
# A retained document is not an independent run, even if a later capture looks
# plausible. Do not swallow a locked file or an incomplete enumeration.
$ErrorActionPreference = 'Stop'
$packages = @(Get-AppxPackage -Name Microsoft.WindowsNotepad -ErrorAction Stop)
if ($packages.Count -ne 1) { throw 'Expected exactly one installed Notepad package' }
$pkg = $packages[0]
Get-Process -Name Notepad -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction Stop
Start-Sleep -Milliseconds 1500
$state = Join-Path $env:LOCALAPPDATA "Packages\$($pkg.PackageFamilyName)\LocalState"
$removed = 0
$remaining = 0
if (Test-Path -LiteralPath $state) {
    foreach ($child in Get-ChildItem -LiteralPath $state -Force -ErrorAction Stop) {
        Remove-Item -LiteralPath $child.FullName -Recurse -Force -ErrorAction Stop
        $removed++
    }
    $remaining = @(Get-ChildItem -LiteralPath $state -Force -ErrorAction Stop).Count
    if ($remaining -ne 0) { throw 'Notepad session reset left retained state' }
}
Write-Output "BVMODERNRESET family=$($pkg.PackageFamilyName) state_present=$(Test-Path -LiteralPath $state) removed=$removed remaining=$remaining success=True"
