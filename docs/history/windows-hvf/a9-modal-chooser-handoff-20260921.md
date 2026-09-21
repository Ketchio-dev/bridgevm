# A9 modal chooser handoff — 2026-09-21

Status: deterministic correction after one failed physical pilot. A9 and A11 remain
OPEN, product state remains Engineering Preview, and 3D remains outside the release path.

## Measured pilot

Exact-main `84e468fa8bbffdffb365707ef8fe9d24e2367939` completed all 45 hosted push
workflows, including core CI run `35591475932`. Its exact-source Apple Development app passed strict signing, entitlement, nested-helper, firmware and LaunchServices Accessibility preflight. The sealed input manifest SHA-256 was
`7022853f8d97f1411af9efedb9e76f9a04d2972a0a6450d01843eb18447d5cbc`.

Physical job `t17-84e468fa-runtime-press-r27` passed artifact preflight, automated the
product UI, created the VM, prepared the source, installed Windows and provisioned Secure Boot with 3D disabled. It retained authenticated final disk, variables and Secure Boot hashes, then failed before first READY while selecting the runtime host share:

```text
failure_code=ui-element-missing
AXPress failed: bridgevm.runtime.share.host.choose;
first_ax_error=-25204; retry_ax_error=-25204;
activation_succeeded=true; frontmost=true; attempts=11
```

Cleanup was verified. Both strict receipt verifiers passed. Public and private receipt SHA-256 are
`a40303f737283e07541a59d17b4691982eadc385a310e1804af881f9352dbbd0`;
the authenticated private lane result SHA-256 is `f32b356675ccd1cd1fac6c666c93c7848ec34d5e003a576b5d2be39870f60f8f`. The job did not complete T17, so no T19 source was retained.

## Corrected boundary

The measured control invokes synchronous `NSOpenPanel.runModal()`. An AX press
can therefore return `AXError.cannotComplete` after the target application has
entered the modal loop. Retrying the same button cannot prove whether the
panel opened and prevents the chooser state machine from observing it.

Source `defcc17dd6af502469b70fcb0511f768b8550415` gives chooser triggers a
separate contract. It resolves the exact identifier with exact AXButton role,
requires a readable enabled state and performs one press. Only `success` and
`cannotComplete` cross into the chooser state machine, and both are merely
provisional. The existing bounded flow must still observe exactly one
`open-panel` with an allowed role, reject ambiguous or unreadable graphs,
complete exact-path selection, observe dismissal and confirm the exact selected
path. Any other AX error fails immediately; `cannotComplete` without a panel
times out as a failure. The raw trigger result is retained in diagnostics.

The BridgeVMProductE2E target compiled. All four XCTest-shim suites passed:
425 BridgeVMApp, 856 BridgeVMControl with two explicit live skips, 62
AppleVzRunnerCore and 149 BridgeVMProductE2E tests. Structural budgets passed
without raising an existing ceiling. The local `swift test` invocation is not
claimed because Command Line Tools do not ship XCTest; the repository shim is
the supported deterministic local venue. Exact-head hosted CI, integration and
a replacement physical T17 pilot remain required.

The first complete-project attempt on metadata head `26e87bec` was stopped at the
repository's 300-second local limit while the shim suites rebuilt. Every completed
executable gate passed. Its two earlier failures were the expected stale
`tested_commit` after the structural-budget row and the initially missing document
classification; both are retained and corrected next. The interrupted shim step is
not reported as a pass or failure. The independent complete shim result above applies;
the complete deterministic project result must come from GitHub-hosted CI.
