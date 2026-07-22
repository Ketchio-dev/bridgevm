# Windows vTPM command path — live evidence (2026-07-22)

Document status: **Historical evidence**

## Outcome

A 120-second no-QEMU HVF run reached the Windows 11 ARM64 desktop with four
vCPUs, the pinned Secure Boot + TPM2 EDK2 firmware, a dedicated swtpm state
directory, virtio-net, and the aggressive Venus 3D lane. BridgeVM's TPM TIS
frontend completed 1,032 guest commands with 975 successful TPM responses,
zero backend failures, and zero malformed commands or responses.

This advances `SEC-TPM-FRONTEND` to partial E4 evidence: Windows exercised the
live ACPI/TIS device and security-runtime command path. It does not close the
gate because the PPI mailbox had 13 reads but no writes, so no physical-presence
operation was requested or completed.

## Reproducible configuration

- Repository base before the telemetry packet: `f5f9ebd`.
- Original Windows disk and UEFI vars were left untouched; the run used new
  APFS clones under
  `/Users/insighton/BridgeVM/work/vtpm-command-proof-20260722*`.
- Firmware: `crates/bridgevm-hvf/firmware/edk2-aarch64-secure-code.fd`, SHA-256
  `f41c7eb7c1a9dabf8ed10c4e52642378e05df171eecd65ca15ed414d9fabdff9`.
- VM: 6,144 MiB RAM, four vCPUs, 120-second watchdog, virtio-net, Venus 3D,
  `performance-risk=aggressive`.
- The state directory was disposable and unencrypted for this frontend proof.
  It is not lifecycle-security evidence.

## Structured receipt

The probe emitted this payload-free summary at shutdown:

```text
TPM2 TIS command summary: commands=1032 success=975 errors=57 backend_failures=0 malformed_commands=0 malformed_responses=0 last_command=0x00000155 startup=1 self_test=1 get_capability=185 pcr_read=146 pcr_extend=81 start_auth_session=186 create_primary=3 read_public=9 nv_read_public=40 get_random=5 other=375
TPM PPI shared-memory summary: reads=13 writes=0 rejected_accesses=0 memory_overwrite_requested=false
```

`StartAuthSession`, `CreatePrimary`, `NV_ReadPublic`, `PCR_Read`, and
`PCR_Extend` traffic establishes substantially more than a register-presence or
firmware-only smoke. TPM error responses are retained as protocol evidence;
the important transport-integrity counters—backend failures and malformed
packets—are both zero.

The 30-second framebuffer was a clean Windows desktop. Its PPM checksum is
SHA-256
`60a82d5e1e82dfd7093d00a5e17cea89fc9d12f4021b155f9d98122afb2cef9e`.
At 60 seconds the image's resident PPSSPP startup attempted Vulkan, failed, and
reported a fallback to D3D11. That negative result belongs to the current GPU
compatibility gate and is deliberately not hidden by this TPM receipt.

## Verification and retained artifacts

The reusable verifier passes the retained directory:

```sh
tests/integration/verify-hvf-windows-vtpm-live-evidence.sh \
  /Users/insighton/BridgeVM/work/vtpm-command-proof-20260722/evidence
```

Key retained hashes:

| Artifact | SHA-256 |
| --- | --- |
| `preflight.txt` | `cb81f856e63e77c6736d2c15e5b3ff3c9979b553a73c0b0c1e14f9f8bcd7b389` |
| `run.log` | `c24f4617774366dba14cae8ef2c26657c37f9adf7f23d674d47abe848780fa08` |
| 30-second desktop PPM | `60a82d5e1e82dfd7093d00a5e17cea89fc9d12f4021b155f9d98122afb2cef9e` |
| 60-second PPSSPP PPM | `4fef0b07d21c3ab81549786ac41248b25f086ea96119a7de7c9861939c5ec479` |

The raw swtpm debug log is retained outside Git because it contains complete
binary TPM packets. The committed structured summary records command codes and
counts only; it never logs TPM payloads.

## Claim boundary

This run does not prove a PPI action, encrypted-state recovery, a clean-second-
Mac migration, `Confirm-SecureBootUEFI`, PCR 7/event-log correctness, BitLocker
recovery, or production GPU compatibility. Those gates remain open.
