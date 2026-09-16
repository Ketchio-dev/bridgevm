# Native app VM creation CLI — 2026-09-16

Classification: historical deterministic evidence. Product state remains in the
[registry](../../../capabilities/windows-hvf.json). A9 and A11 stay OPEN and
ENGINEERING_PREVIEW is unchanged.

## Implemented boundary

Source `9be8d8848182dc19a711c07d696d319c6ae62e37` adds
`bridgevm app create-windows NAME --iso ABSOLUTE_PATH`. The command forwards an
exact argument vector to the installed native app executable. The native owner
then uses the same `VMLibrary.createWindowsHVFInstall` transaction as the app's
creation sheet, so both paths publish the same own-HVF, install-pending library
record and managed ISO copy.

The command accepts the app's existing disk, memory, CPU, resolution and network
choices. It rejects duplicate or unknown arguments, empty or whitespace-altered
names, relative or parent-traversing paths, directories, final-component ISO
symlinks, unreadable media, unsupported resource values and CPU counts beyond
the host policy. Help is read-only. A VM whose canonical ID is literally
`create-windows` still routes through the ordinary VM command path unless the
actual create subcommand position is present.

Publication remains atomic through the shared library transaction. After
creation, the native CLI reads the saved record back and verifies its canonical
ID, own-HVF backend, Windows-HVF boot mode, pending installation request,
resource choices and normalized configuration digest before reporting success.
The versioned `bridgevm.app-create-windows.v1` JSON contains the canonical ID,
saved settings and digest. It does not expose the source or managed ISO path,
media identity, password, recovery key or other secret material.

This command creates the registration only. Installation remains a separate
`bridgevm app install ID` operation, and starting remains a separate lifecycle
operation after installation reaches a valid terminal state.

## Retained validation

| Check | Result |
| --- | --- |
| Rust CLI unit suite | 148 PASS |
| Rust real-process app-create integration | 2 PASS |
| Other Rust app CLI integration suites | 17 PASS across install, start, stop, list/inspect, doctor and help |
| Focused native parser, creation and failure suites | 9 PASS |
| Existing repository Swift harness | 16 PASS through `scripts/run-swift-tests.sh` |
| Rust formatting and structural budgets | PASS without raising an existing ceiling |
| Compiled native executable with a unique synthetic ISO | PASS; created and immediately inspected canonical ID `cli-검증-vm` with an install-pending own-HVF record |

The first Rust compilation failed because the new clap argument type omitted
the `Args` trait import. The import was added and the complete CLI package then
passed. That failure remains part of the record.

A raw, unsupported `swift test --package-path apps/macos` invocation produced
70 failures, including shared-state failures in unchanged suites. A singled
unchanged product-policy test also failed because it lacked the repository's
isolated library setup. These results are retained and are not treated as
product regressions or rewritten as successes. The supported focused Swift
selection and repository Swift harness above both passed; the final integrated
project gate is recorded separately at the documentation checkpoint.

## Evidence limit

The synthetic-media run proves native parsing, managed host registration,
read-back validation and inspectability. It does not validate a Windows ISO,
install Windows, launch App.main, render a WindowServer UI, boot or stop a guest,
or prove recovery on physical hardware. No live queue job was submitted because
this packet is deterministic. Exact-head hosted validation remains required
before integration. No criterion, threshold, product wording, known defect or
machine-contract deviation is promoted here.
