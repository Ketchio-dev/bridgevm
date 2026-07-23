# Windows ARM64 encrypted vTPM migration to a second Mac (2026-07-23)

## Result

`SEC-TPM-LIFECYCLE` is live-proven for the self-controllable migration boundary.
The packaged BridgeVMControl app booted the source VM on Mac Studio [A] with an
encrypted swtpm state, exported an authenticated recovery package, transferred
the app/package/state/vars/disk to the MacBook Pro M5 [B], restored the same
stable VM identity, and reached the Windows desktop on [B] without a recovery
prompt. The restored boot completed 239 `PCR_Read` operations with zero backend
or malformed TPM failures and shut down cleanly through the resident guest
agent and PSCI.

The BitLocker-specific clone contrast remains blocked by the guest edition:
`EditionID=Core` is Windows 11 Home, and no Pro/Enterprise upgrade key was
available. C3 and C6 are therefore `BLOCKED_BY_EDITION`, not product failures.

## Packaged runtime (C4)

Latest package:

```text
~/BridgeVM/packages/BridgeVMControl-final2-20260723.app
BridgeVMControl sha256=56ec73abd3a2d4c49619e917fc3bcce7c883d90b394ad4cef5df2f35a8f22ead
swtpm sha256=36b66345548f8d420cff550222a1f9c73a376dd6a61da69decd516b8be40e82b
swtpm_version=0.10.1
libtpms_version=0.10.2
state_encryption=aes-256-cbc-etm/key-fd
```

`codesign --verify --deep --strict` passed on both [A] and [B]. The final
standalone packaged-runtime smoke is in
`~/BridgeVM/runs/wall-c4-final2-swtpm-smoke-20260723/result.txt`:

```text
standalone_key_fd=true
data_socket_created=true
control_socket_created=true
encrypted_state_created=true
cleanup_process=true
TPM emulator version 0.10.1
```

The 32-byte smoke key was generated with the OS CSPRNG and passed to the
packaged helper as binary stdin only. No key was written to argv, environment,
or a key file.

The lifecycle CLI also received security hardening before this receipt:
existing-state enumeration now fails closed, recovery outputs use exclusive
`O_NOFOLLOW` creation with mode `0600`, and the production CLI no longer exposes
a file-keychain downgrade. Targeted lifecycle/security tests passed 11/11 on
[B]; the [A] production target builds, while [A]'s full Swift test command is
blocked by its existing SDK `no such module XCTest` mismatch.

## Source export on [A]

Evidence: `~/BridgeVM/runs/wall-c5-encrypted-source-20260723/`

```text
stable VM ID=wall-run-20260723
state_encryption=aes-256-cbc-etm/key-fd
BVAGENT READY
TPM commands=724 success=669
PCR_Read=115 PCR_Extend=82
backend_failures=0
malformed_commands=0 malformed_responses=0
stop=PSCI system off
```

Exported recovery package:

```text
format=bridgevm-vtpm-recovery-v1
stableVMID=wall-run-20260723
stateFingerprint=a670fa21abb9276e0b8e9c08a42c43d08ac91a14ef09abeb13f85152f2860fd0
package sha256=8225e0a3cbe3b4039de98fac3b1f30abdf257509f88baa5dc10175471b426802
```

The recovery code is stored only in private mode-`0600` files and is not
included in this document.

## Transfer integrity

The 48 GiB Windows disk copied from [A] to [B] byte-for-byte:

```text
[A] wall-run-20260723.raw sha256=ba7a22bb95dad289a4bd3138d2cc3374f3beda7365e9a8e5de3256880ef737bc
[B] wall-run-20260723.raw sha256=ba7a22bb95dad289a4bd3138d2cc3374f3beda7365e9a8e5de3256880ef737bc
```

The recovery package hash on [B] is the same
`8225e0a3cbe3b4039de98fac3b1f30abdf257509f88baa5dc10175471b426802`.
The UEFI vars hash was
`8220da8c67c303e00af955a652d8e0d89268952ef20a5170f6f8dbaf7cfeea7b`.

## Same-ID restore and boot on [B] (C5)

Machine [B] is an Apple M5 Pro MacBook Pro. It used only the transferred,
strictly verified packaged app and recovery inputs; no local source build or
Homebrew swtpm was used.

Evidence:
`~/BridgeVM-B/wall-run-20260723/logs/c5-same-id-restore-boot-20260724/`

```text
BVAGENT READY host=BRIDGEVM v3-share2
BVAGENT CMD whoami exit=0
state_encryption=aes-256-cbc-etm/key-fd
TPM commands=626 success=621 errors=5
PCR_Read=239 PCR_Extend=82
backend_failures=0
malformed_commands=0 malformed_responses=0
BVAGENT CMD shutdown.exe /p /f exit=0
stop=PSCI 0x84000008 (system off)
NVMe disk written back
cleanup_status=0
```

The 60-second framebuffer shows the Windows desktop, not a BitLocker recovery
screen. A PPSSPP graphics-backend dialog from the unrelated 120.43 GPU lineage
was visible, but it does not affect the same-ID vTPM migration result.

## Edition boundary (C3/C6)

The installed guest reports `EditionID=Core` (Windows 11 Home). The requested
BitLocker protector/no-prompt reboot and fresh-ID recovery-screen contrast
require Pro/Enterprise functionality. With no owner-supplied upgrade key, C3
and C6 remain explicitly blocked by edition. The self-controllable migration
security claim is limited to authenticated recovery, stable-ID preservation,
encrypted state, TPM command continuity, and successful same-ID boot.
