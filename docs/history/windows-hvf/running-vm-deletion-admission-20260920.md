# Running-VM deletion admission

## Removed side effect

Library deletion previously scheduled `VMBackend.stop()` before removing the
entry. For the historical HVF backend that method discovers processes from the
target disk path and can eventually kill them. Deletion could therefore cross
the exact runtime-ownership boundary even after the visible Stop controls had
been restricted to an app-owned identity.

Source `f3ee12430581dd978f776307146c7a6ec589b47f` makes deletion fail closed:

- the final worker probes liveness once and refuses a running VM without a
  stop call, guest-control write or delete attempt;
- an own-HVF deletion acquires the native disk-and-vars media lease before
  removing the library entry and keeps that lease through the operation;
- failure to acquire or retain native ownership is reported as deletion
  failure; and
- Fast VZ and QEMU retain their registration deletion path after the same
  stopped-state check, so the own-HVF lease requirement does not disable other
  engines.

The existing `deletingSlugs` reservation remains active for the complete
worker lifetime. App-owned install, runtime and file actions therefore cannot
start concurrently while deletion admission is in progress.

## Deterministic verification

Three focused tests prove that a running observation never invokes the delete
closure, a stopped observation reports the exact delete result, and only the
own-HVF engine selects native media-lease protection. The four Swift shim
suites pass with 425 BridgeVMApp tests, 839 BridgeVMControl tests with the two
required live-only skips, 62 AppleVzRunnerCore tests and 109
BridgeVMProductE2E tests. Structural budgets pass; `LibraryModel.swift` falls
from 177 to 172 lines and every new file is registered at its actual size.

The complete source-tree project check ran all executable, app, security,
documentation and structural steps. Rust reported 1,018 own-HVF tests passing
with one existing intentional ignore and 381 probe tests passing. Its only
failed step was the expected capability-freshness gate because `tested_commit`
still named the preceding checkpoint before this record. The retained
7,706-line log SHA-256 is
`2947c3270b8cbe3cab8c81b0d123e28deea86cdf23ad378d6d0ae0b349930b94`.

This checkpoint proves deterministic host-side deletion admission and native
media ownership. It does not prove live Windows deletion, shutdown, guest
flush, interruption recovery, release readiness, performance or graphics
behavior. A9, A11, A19 and B6 remain OPEN, product state remains Engineering
Preview, and 3D remains outside the release path. The running user VM was not
stopped or mutated.
