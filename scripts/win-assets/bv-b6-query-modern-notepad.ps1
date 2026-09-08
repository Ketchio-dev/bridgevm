# Diagnostic-only guest asset for B6 scene-composition scoping. Not part of
# any shipped closure gate.
$ErrorActionPreference = 'Stop'
$pkg = Get-AppxPackage -Name Microsoft.WindowsNotepad -ErrorAction SilentlyContinue
if ($pkg) {
    Write-Output "BVNOTEPADPKG version=$($pkg.Version)"
} else {
    Write-Output 'BVNOTEPADPKG absent'
}
