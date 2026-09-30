# A9 r49 input stages pass, audio stops — failed physical diagnostic

Classification: one failed, nonpromoting physical diagnostic. The current A9
criterion, defect wording and product state come from
`capabilities/windows-hvf.json`; A9 remains OPEN and the product remains
ENGINEERING_PREVIEW.

## Sealed observation

Physical job `t17-4c1b470f-click-beside-start-pilot-r49` ran
`4c1b470fc4c556df9b0671f13dbff84ff7397177`, the PR #278 head carrying the click
beside the first-logon Start menu that followed
[r48](a9-r48-start-menu-not-foreground-20260930.md). Its full project check
`36691746606` succeeded. The app was a local exact-source Apple Development
build, the Accessibility preflight and console check passed, and the input
manifest SHA-256 was
`dd0a6d1eef25433e78c8c0af7dd2a472176c5f46cc2599a03067f5466848c926`.

The job ran on Mac17,9, macOS 27.0, from 08:48:43 to 09:05:52 UTC and failed
with `integration-failed`. Byte-identical receipts (SHA-256
`64c338c763c1d1e0c7555fbfc855330bfdc2db8277376ee4d4244da10d7d5a43`) passed the
strict read check, the lane result SHA-256 was
`5b6f9a6b1e9359b174e2df0136445b1cf680ee7621927b76752666c2bd2b9f36`, and worker
cleanup was verified. All 37 half-minute samples found the console unlocked,
and the product app was frontmost in 33 of the 35 taken while the job ran.

Installation, Secure Boot provisioning and first READY each passed once, the
twelfth consecutive first READY. Then, for the first time on hardware, the
lane result records four journey stages as passed:
- **Keyboard and pointer:** after the click beside the Start menu and the
  click on the form, the form's progress changed and it wrote its nonce-bound
  output.
- **Clipboard:** the round trip through the guest clipboard.
- **Folder share:** both directions.
- **Network.**

The audio workload was launched and produced no output within 120 seconds.
The post-READY host diagnostic stop completed. The frozen screen shows a
clean Windows desktop, and the host rendered audio frames with no drops.

## Reading

The audio workload writes its tone to `C:\ProgramData\BridgeVM\t17-tone.wav`
without creating that directory, and nothing in product provisioning creates
it; the B7 audio script creates it first. On a freshly product-installed
guest the write would throw before playback, and the snapshot-marker stages
that write into the same directory would fail the same way. This is the
leading explanation by static reading. r49 retained no guest error that
confirms it.

## Follow-up

The guest workload now creates `C:\ProgramData\BridgeVM` before any action.
A failed action writes `t17-error-<prefix>.txt` naming the action and its
exception type, and still fails. Every T17 workload wait reports that line
after strict parsing when the output never arrives. None of this has run on
hardware; A9 and A11 remain OPEN and product state stays ENGINEERING_PREVIEW.
