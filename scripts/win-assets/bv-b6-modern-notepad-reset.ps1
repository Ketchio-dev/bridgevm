# Diagnostic-only guest asset for the B6 glyph matrix. Not part of any shipped
# closure gate. Packaged Notepad restores the previous session's tabs and their
# text when it is relaunched -- its own first-run tip advertises this -- so
# without a reset each B6 run types into the document the last run left behind
# and the runs are not independent, which is exactly what the criterion's
# "3 independent runs" requires them to be. Observed live on 2026-09-09: three
# runs accumulated into one document rather than producing three.
#
# The session lives in the package's LocalState. Stopping the app and removing
# that state is the documented-shape reset; it touches only this package's own
# data inside a disposable test guest.
$ErrorActionPreference = 'Stop'
$pkg = Get-AppxPackage -Name Microsoft.WindowsNotepad -ErrorAction Stop
Get-Process -Name Notepad -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
Start-Sleep -Milliseconds 1500
$state = Join-Path $env:LOCALAPPDATA "Packages\$($pkg.PackageFamilyName)\LocalState"
$removed = 0
if (Test-Path -LiteralPath $state) {
    foreach ($child in Get-ChildItem -LiteralPath $state -Force -ErrorAction SilentlyContinue) {
        try {
            Remove-Item -LiteralPath $child.FullName -Recurse -Force -ErrorAction Stop
            $removed++
        } catch {
            # A file the app still holds open is not a reason to fail the run;
            # report the count and let the caller judge the reset by the first
            # capture's character count.
        }
    }
}
Write-Output "BVMODERNRESET family=$($pkg.PackageFamilyName) state_present=$(Test-Path -LiteralPath $state) removed=$removed"
