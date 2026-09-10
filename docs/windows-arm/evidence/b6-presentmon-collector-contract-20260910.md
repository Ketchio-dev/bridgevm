# B6 PresentMon collector contract, 2026-09-10

B6 remains OPEN. This is collection-tool hardening, not a matrix cell or a
frame-time comparison. The original resolutions, effective scales, independent
runs, reviewed masks and frame-time-within-10%-of-baseline requirement remain.

## Source-level defects and scope

The old collector supplied an unquoted output path in Start-Process's argument
array. PowerShell joins that array into a command line, so spaces need explicit
native quoting. This follows the [Windows PowerShell Start-Process contract](https://learn.microsoft.com/en-us/powershell/module/microsoft.powershell.management/start-process?view=powershell-5.1),
not an assertion that the old retained captures failed because of path spaces.
It also ignored deletion failures for an existing CSV and used
`--stop_existing_session` with PresentMon's default session name.

The updated collector refuses existing output rather than deleting evidence,
quotes the full output path, and assigns a unique session name without stopping
another session. PresentMon documents independent session names and explicit
session-stop options in its [pinned v2.5.1 console contract](https://github.com/GameTechDev/PresentMon/blob/v2.5.1/README-ConsoleApplication.md).
The host's existing pinned-binary verification remains required before staging.
The guest asset keeps CRLF line endings.

It bounds collection to 1--300 seconds plus a 30-second exit grace, refuses a
missing/empty/oversized CSV, requires v2 FrameTime/Application columns and at
least one data row, and emits the collected file's SHA-256 and row count.
An owned process still running on exit is killed and given a bounded exit wait.
Neither row presence nor this hash proves comparable frame timing, sufficient
sample coverage, glyph correctness, ETW cleanup, or any B6 criterion pass.

## Native deterministic contract test

The added GitHub-hosted Windows workflow builds a small local C# process fixture
and executes the real PowerShell collector. It checks native argument delivery
through a path with spaces, distinct session names, preservation of old output,
refusal of empty/missing/oversized/wrong-column results, invalid duration/path
arguments, nonzero exit, and termination of its owned timed-out process.
The fixture is not PresentMon and its invented rows never enter live receipts.
It needs neither private Windows media nor a physical Mac, so it runs only on
GitHub-hosted Windows infrastructure. This Mac has no PowerShell installation;
local project checks cannot establish the native Windows contract.

Actual PresentMon/ETW behaviour, a changing real scene, a comparable baseline,
the complete glyph matrix and release-tier receipts are still required.
