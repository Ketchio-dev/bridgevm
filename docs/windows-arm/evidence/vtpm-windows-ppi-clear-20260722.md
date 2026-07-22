# Windows vTPM PPI clear action — live evidence (2026-07-22)

Document status: **Historical evidence**

## Outcome

A no-QEMU HVF run drove a full Windows 11 ARM64 TPM Physical Presence Interface
(PPI) **clear** to completion inside a single VM host process: Windows requested
the clear, rebooted, the pinned firmware displayed its caution prompt, an F12
physical-presence approval was delivered, the firmware executed a real
`TPM2_CC_Clear`, reset itself, and Windows returned to the desktop. The run then
shut down cleanly.

This closes the previously-open PPI-action half of `SEC-TPM-FRONTEND`. The
earlier [command-path receipt](vtpm-windows-command-path-20260722.md) proved the
live TIS command flow but recorded zero PPI writes and no physical-presence
operation.

## Two defects this receipt required fixing

Both prior PPI-action attempts reached the firmware caution prompt and delivered
F12, but the firmware's `TPM2_ClearControl` was rejected with `TPM_RC_BAD_AUTH`
and no `TPM2_CC_Clear` was ever issued. Root cause was a pair of platform-
authorization problems:

1. **vTPM not power-cycled on guest reset (device model).** On PSCI
   `SYSTEM_RESET` the supervised swtpm kept its in-memory volatile state, so the
   platform authorization the firmware randomizes at end-of-boot persisted into
   the next firmware generation. `SwtpmUnixBackend` now issues swtpm's
   control-channel `CMD_INIT` (a `_TPM_Init` power cycle) on reset, resetting
   volatile state — platform authorization, PCRs, sessions — while preserving
   persisted permanent state, exactly as a hardware TPM does on system reset.
   The launcher passes the swtpm control socket to the probe for this purpose.

2. **PPI processed after the platform hierarchy was locked (firmware).**
   ArmVirt's light boot manager called `ConfigureTpmPlatformHierarchy ()` (which
   randomizes and discards `platformAuth`) in `PlatformBootManagerBeforeConsole`,
   and the previous patch processed the PPI request later, in
   `PlatformBootManagerAfterConsole`. The firmware then authorized the clear with
   an empty `platformAuth`, which no longer matched. The firmware patch now
   connects the default consoles and processes the PPI request at the end of
   `PlatformBootManagerBeforeConsole`, **before** locking the hierarchy — mirroring
   OvmfPkg's full `PlatformBootManagerLib`.

With both fixes the firmware's `TPM2_ClearControl` and `TPM2_CC_Clear` both
returned `TPM_RC_SUCCESS`.

## Reproducible configuration

- Repository base: `629e8eb` (branch `sec-tpm-frontend/ppi-action-receipt`).
- Original Windows disk and UEFI vars were left untouched; the run used fresh
  APFS clones of the `vtpm-command-proof-20260722` disk, UEFI vars, and swtpm
  state, plus a fresh evidence directory.
- Firmware: `crates/bridgevm-hvf/firmware/edk2-aarch64-secure-code.fd`, SHA-256
  `b1dc201b1382476ca8c8dcbf8c09abc7ae7429c8437e35bffd54bb9b228b750b`, a
  reproducible commit-pinned build with Secure Boot + TPM2 enabled and the
  updated PPI patch
  (`crates/bridgevm-hvf/firmware/patches/0001-armvirt-process-tpm-ppi.patch`,
  SHA-256 `400493912254bfe03336a8112b7fb56a42ca8bae9610d6c1bda92d250c046b14`).
- VM: 6,144 MiB RAM, four vCPUs, xHCI HID enabled for live input, virtio-net off,
  `performance-risk=balanced`. The swtpm state directory was disposable and
  unencrypted for this frontend proof; it is not lifecycle-security evidence.

## Guest-visible flow (payload-free)

Every guest command was chosen to avoid printing any TPM secret. `Clear-Tpm`'s
result object (which includes `OwnerAuth`) was discarded with `| Out-Null`;
owner-authorization state was reported only as the boolean `![bool](Get-Tpm).OwnerAuth`,
never as a value.

| Stage | Guest observation |
| --- | --- |
| Pre-clear | `TpmPresent=True`, `TpmOwned=True`, `TpmReady=True`, `!OwnerAuth=False` (owner auth present) |
| Request | `Clear-Tpm -UsePPI` → `RestartPending=True` |
| Reboot 1 | `shutdown.exe /r /t 0` → PSCI `SYSTEM_RESET` (in-process) |
| Firmware | TianoCore "clear this computer's TPM" caution prompt; F12 delivered live between the two resets |
| Reboot 2 | firmware-triggered post-clear PSCI `SYSTEM_RESET` |
| Post-clear | `RestartPending=False` (request consumed), `TpmReady=True`, `TpmPresent=True` |
| Shutdown | `shutdown.exe /s /t 0` → clean PSCI `SYSTEM_OFF` |

Windows TPM auto-provisioning re-establishes ownership after a clear, so
`TpmOwned` returning to `True` post-clear is expected real behavior; the proof of
the clear is the executed `TPM2_CC_Clear` and the `RestartPending` transition,
not a permanently blank owner authorization.

## Structured receipt

The probe emitted this payload-free summary at shutdown:

```text
TPM2 TIS command summary: commands=1418 success=1357 errors=61 backend_failures=0 malformed_commands=0 malformed_responses=0 last_command=0x00000145 clear=1 startup=3 self_test=3 get_capability=570 pcr_read=292 pcr_extend=174 start_auth_session=84 create_primary=3 read_public=15 nv_read_public=71 get_random=10 other=192
TPM PPI shared-memory summary: reads=32 writes=266 rejected_accesses=0 memory_overwrite_requested=false
```

`clear=1` (up from `0` in every prior run) and `writes=266` are the load-bearing
counters. Transport integrity is intact: `backend_failures`, `malformed_commands`,
`malformed_responses`, and PPI `rejected_accesses` are all zero. The TIS counters
are cumulative across the three in-process boot generations; the PPI summary
reflects the final generation.

The swtpm backend log (retained outside Git because it contains complete binary
TPM packets) independently confirms the firmware clear: exactly one
`TPM2_ClearControl` (`0x00000127`) and one `TPM2_CC_Clear` (`0x00000126`) were
received during the second boot generation, each answered with response code
`0x00000000` (`TPM_RC_SUCCESS`). Only command and response codes are recorded
here; no TPM payloads are reproduced.

## Verification and retained artifacts

The reusable verifier passes the retained directory in its PPI-action mode:

```sh
tests/integration/verify-hvf-windows-vtpm-live-evidence.sh \
  /Users/insighton/BridgeVM/work/vtpm-ppi-clear3-20260722/evidence-ppi-action \
  --ppi-action
```

It requires `clear>0`, PPI writes, a first (Windows) and second (firmware) reset,
an F12 acceptance strictly between the two resets, a captured post-clear
framebuffer checkpoint, a clean guest `SYSTEM_OFF`, and no replayed, malformed,
or misdelivered live input.

Key retained hashes (artifacts kept outside Git):

| Artifact | SHA-256 |
| --- | --- |
| `preflight.txt` | `3977cf2d41349b9d60ac0d00dabcebbbd303534cc8d4837e3c7a2f3585b3a717` |
| `run.log` | `8ac3616f17ffef1f48545ed320f545e951befb66609832a258c3d0f89ad032b0` |
| `target-stat.txt` | `f3812ac23f9e66a4135847100109a74069a437d7492c78bf2f86627d69361b7b` |
| `cleanup.txt` | `32ccedf076b271a15fef3576ad3273eb6b056f2779401bcca2d151b9743eb4af` |

The raw swtpm debug log, live-input control file, and framebuffer captures are
retained outside Git; the framebuffer captures show only the boolean TPM state
above and the firmware caution prompt, never owner authorization or other
secrets.

## Claim boundary

This run proves a live Windows PPI clear action end to end. It does not prove
encrypted-state recovery, a clean-second-Mac migration, `Confirm-SecureBootUEFI`,
PCR 7 / event-log correctness, BitLocker recovery, or production GPU
compatibility. Those gates remain open.
