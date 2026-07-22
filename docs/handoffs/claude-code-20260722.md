# Claude Code handoff — 2026-07-22

Document status: **Active handoff**

This document is the exact continuation boundary for the next coding agent.
Do not infer completion from locally implemented plumbing: release gates close
only when their dated verifier-backed live evidence exists.

## Repository state

- Branch: `codex/live-input-snapshot`
- Base commit: `bcb809c582db50ac7cbdcd2c46d9078aef2adee3`
- `CLAUDE.md` is an unrelated untracked user file. Do not modify, delete, add,
  or commit it.
- This dated handoff records the packet before integration. Check `git log` and
  the current branch before continuing; do not assume the base commit is still
  checked out.

## Current packet

The working tree contains one coherent Windows vTPM/PPI observability packet:

- live `SNAPSHOT <label>` input with bounded framebuffer evidence;
- live-input offset/pending-queue lifetime moved outside the guest reboot loop,
  preventing destructive command replay after `PSCI SYSTEM_RESET`;
- named F1–F12 boot-key support, including the EDK2 TPM approval key F12;
- payload-free `TPM2_CC_Clear` command counting;
- a reproducible EDK2 patch that makes ArmVirt's light boot manager process PPI
  requests and reads/writes packed 32-bit PPI fields through volatile bytes;
- rebuilt firmware SHA-256
  `7658b515e644620a0d51a9bf1ce43541cef019b7fcccf8087cd0244840a9cb4d`;
- backward-compatible evidence parsing for older receipts without `clear=`.

The firmware patch is
`crates/bridgevm-hvf/firmware/patches/0001-armvirt-process-tpm-ppi.patch`.
Its SHA-256 is recorded in the firmware build receipt.

## What live evidence proves

A 20-second disposable-clone run with the patched firmware completed cleanly:

```text
TPM2 TIS command summary: commands=483 success=480 errors=3 backend_failures=0 malformed_commands=0 malformed_responses=0 last_command=0x0000017b clear=0 startup=1 self_test=1 get_capability=186 pcr_read=115 pcr_extend=81 start_auth_session=24 create_primary=0 read_public=6 nv_read_public=12 get_random=4 other=53
TPM PPI shared-memory summary: reads=20 writes=276 rejected_accesses=0 memory_overwrite_requested=false
```

It also had successful real NVMe I/O and no synchronous firmware exception.
The local receipt is under
`/Users/insighton/BridgeVM/work/vtpm-ppi-action-20260722/evidence-ppi-byte-access-firmware-receipt`.

An interactive run visually showed `Clear-Tpm -UsePPI` returning
`RestartPending=True` with PPI version 1.3. That screen also displayed TPM
owner authorization material and must never be committed or published. The run
was interrupted after exposing the reboot command-replay bug, so it is not a
valid completion receipt.

## Immediate next action

Finish `SEC-TPM-FRONTEND` on the disposable clone, in one VM process:

1. Run `scripts/run-hvf-windows-installed-boot.sh` with `--enable-xhci`,
   `--input-control`, the existing disposable raw disk/vars/state paths, and a
   fresh evidence directory.
2. At the Windows desktop, append only to the end of the control file: close
   the PPSSPP warning, open elevated PowerShell, and run
   `Clear-Tpm -UsePPI`.
3. Record a bounded result snapshot locally, but do not publish any frame that
   contains `OwnerAuth`.
4. Run `shutdown.exe /r /t 0` without leaving the host process.
5. Confirm that old input commands are not replayed after reset.
6. At EDK2's clear warning, append `KEY f12`; EDK2 defines F12 as the caution
   approval key and Escape as rejection.
7. Require `clear>0`, PPI writes, no malformed/backend failures, a successful
   firmware-triggered reset, Windows returning to the desktop, post-clear
   `Get-Tpm` with blank owner authorization, and clean guest shutdown.
8. Extend the verifier with an explicit PPI-action mode and add a dated,
   payload-free evidence document. Only then change `SEC-TPM-FRONTEND` to
   complete.

Disposable inputs currently used:

```text
/Users/insighton/BridgeVM/work/vtpm-ppi-action-20260722.raw
/Users/insighton/BridgeVM/work/vtpm-ppi-action-20260722-vars.fd
/Users/insighton/BridgeVM/work/vtpm-ppi-action-20260722/state
```

## Remaining release walls after the PPI action

Priority order:

1. `SEC-TPM-LIFECYCLE`: clean-second-Mac encrypted state migration plus real
   BitLocker recovery behavior.
2. `SEC-SB-MEASURED`: guest `Confirm-SecureBootUEFI`, PCR 7, event-log, reboot,
   migration, and recovery receipts.
3. `GPU-WDK-SIGN`: fresh ARM64 WDK build, catalog, trusted production
   signature, and clean bind. This requires external credentials/tooling.
4. `GPU-LIVE-RECEIPT`: resolve the PPSSPP Vulkan startup/crash or prove the
   intended fallback, then collect same-boot bind/title/performance evidence.
5. `DIST-MACOS`: Developer ID, hardened runtime, notarization, and clean-Mac
   install/launch. No local code change can manufacture those credentials.
6. Structural debt: split the 34k-line HVF `lib.rs`, then the 13k-line API
   `lib.rs`, through behavior-preserving extraction. Add CI non-increase budgets
   for large-file lines and `unsafe` sites before lowering them. Never mix this
   refactor with a live security/device fix.

## Required checks before PR

```sh
cargo fmt --all -- --check
cargo test -p bridgevm-hvf --example hvf_gic_boot_probe
cargo test -p bridgevm-hvf command_stats_classify_security_runtime_operations_without_payload_logging
tests/integration/hvf-windows-vtpm-live-evidence-verifier-smoke.sh
bash -n scripts/build-hvf-edk2-secure-firmware.sh
bash -n scripts/run-hvf-windows-installed-boot.sh
```

Then run the broader `cargo test -p bridgevm-hvf --all-targets`; on macOS one
socket test may require permission outside the workspace sandbox. Do not commit
raw swtpm packet logs, control files, TPM secrets, or unrelated user files.
