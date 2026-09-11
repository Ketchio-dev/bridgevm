# WinPE binary comparison dependency failure

## Observed failure

Physical job `t7-52fe7304-reference-inventory-staged-agent-r2` failed firstboot
readiness. Its final injection framebuffer shows successful DISM installation,
then `fc` not recognized, then package-local cleanup verification failure.
The injector stops before GPU activation registration. The public receipt
remains failed; it is not rewritten or promoted by this investigation.

Read-only inspection of the retained pre-proof image found the activation
payload files but no matching service or RunOnce entry in the inspected hives.
This supports the displayed failure; it does not replace live guest evidence.

## Repair candidate and limits

The failed run used an old injector image. Source commit `7f31bfc8` had already
replaced `fc` with copy-status, size and header checks on September 7.
The initial investigation incorrectly treated the image's script as current
source and incorrectly described an unrelated source change. That claim is retracted.

The new integration preserves those checks and adds `bv-file-compare.exe` at
an explicit bundled path, requiring complete byte equality without I/O errors.
The portable C implementation returns nonzero for differences or read failures.
Native contract cases cover empty files, chunk boundaries, binary data, length
differences and missing inputs. A source-wiring contract checks the call and
failure guard. A hosted Linux/Windows workflow runs both contracts.

Local native cases and an ARM64 Windows cross-build passed during development.
Neither proves execution in WinPE. A newly built, sealed injector and live run
are still required. Old injector images do not acquire this fix automatically.
Firstboot, B6 and release capability criteria remain unchanged and unproven.
