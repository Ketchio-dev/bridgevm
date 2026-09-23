# A9 selection-confirmation AX read — 2026-09-21

Status: one failed exact-main physical pilot and one bounded deterministic
correction. A9 and A11 remain OPEN, product state remains Engineering Preview,
and 3D remains outside General Preview and v1.

## Exact-main pilot

Merge `15e0caa5c9fec938b44f7dcb229d0efd73c04c66` completed its hosted push
workflows, including CI `35637734888` and Security and quality `35637734244`.
The exact-source Apple Development app passed deep strict signing, Hypervisor
entitlement, nested-helper, firmware, wimlib, swtpm, notice and LaunchServices
Accessibility preflight. Its installed tree SHA-256 was
`e79d12322872bc2e2e51f4a75e5220e60cbeee26892900cc2b360bd285f8cb36`;
the sealed input manifest SHA-256 was
`bbfbecc78eae7aa0ff6fbc78e72a025dcf8aa1b1bd50d95130d18cd01285e456`.

Physical pilot `t17-15e0caa5-modal-handoff-r28` passed artifact preflight,
automated the product UI, created the exact VM, prepared its source, installed
Windows and provisioned Microsoft-only Secure Boot with 3D disabled. It opened
the runtime host-share chooser and advanced through exact path entry and Go To
sheet dismissal, then failed before first READY at:

```text
failure_code=input-selection-failed
stage=selection-confirmation;
file chooser AXIdentifier read failed; ax_error=-25200
```

The lane retained authenticated final disk, variables, Secure Boot and guest
evidence hashes. Cleanup removed its writable lane and per-job build cache. The
strict private and public receipts both passed and share SHA-256
`06292315f260a88040d0f94fdaec2a33a68592f8ecdedfa14c66a2cb8bac1218`;
the authenticated lane result SHA-256 is
`f9ed7def79c793e59e2c626ebdcc1e9b7cc98b526f54cae5d1c72ff89599d78d`.

## Bounded correction

Source `ff1c8e4ffe41be47f2503301378f5dc13784cdb9` preserves the existing
ten-attempt complete AX snapshot and the unchanged whole-chooser deadline.
When that inner snapshot exhausts on the already classified transient generic,
invalid-element or cannot-complete read statuses, the outer wait may reacquire
within the original deadline. Every other failure still propagates immediately.
Success still requires the panel to be absent and the product field to contain
the exact selected path before the deadline. Exhaustion now retains the final
transient error plus the bounded, path-free AX state.

The first shim run caught and retained a real deadline regression in the initial
implementation: readiness observed at or after the deadline was incorrectly
accepted. The corrected ordering restored the prior rule. Structural budgets
then passed without raising an existing ceiling, and all four shim suites passed:
425 BridgeVMApp, 856 BridgeVMControl with two explicit live skips, 62
AppleVzRunnerCore and 152 BridgeVMProductE2E tests.

## Limits

This is deterministic host evidence until a newly signed exact-source artifact
passes another physical pilot. The failed r28 run did not prove chooser
dismissal, selected-path commitment, Start admission, READY/PONG, integration,
shutdown, snapshot/restore, installed-disk import or A9. A later successful
pilot remains one live run and cannot satisfy a larger fixed sample count.
