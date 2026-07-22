# Windows-on-Apple-Silicon architecture and risk policy

Status: adopted 2026-07-22

This note turns the public product behavior of QEMU, VMware Fusion, and
Parallels Desktop into BridgeVM design constraints. It is not a claim that the
closed products use any particular private implementation.

## What the comparison changes

| Product surface | Publicly observable contract | BridgeVM consequence |
| --- | --- | --- |
| QEMU ARM `virt` | `tpm-tis-device` is a sysbus device. Current ARM ACPI exposes `TPM0` as `MSFT0101`; Windows 11 ARM also needs the TPM Physical Presence Interface (PPI). | Follow the Windows-visible ACPI/TIS/PPI contract, but do not copy QEMU's process topology when a tighter in-process or supervised backend is faster. |
| VMware Fusion | Windows 11 ARM uses UEFI, vTPM 2.0, and VM encryption. Fusion 13.5+ exposes DirectX 11 on Apple silicon; an earlier 3D translation crash had an explicit “disable 3D acceleration” recovery path. | Bind TPM state cryptographically to the VM identity. Treat 3D as a replaceable launch policy, never as irreversible guest-media state. |
| Parallels Desktop | Adding vTPM automatically enables Secure Boot. TPM state is an encrypted per-VM file whose password is held in macOS Keychain. New Windows 11 VMs receive vTPM automatically. DirectX 11 is translated through Metal. | Ship vTPM + Secure Boot as one lifecycle. Store the TPM state key in Keychain, keep encrypted state in the VM bundle, and make import/move semantics explicit. Keep the Metal-native scanout path in the performance lane. |

Primary references:

- [QEMU TPM device documentation](https://qemu.readthedocs.io/en/master/specs/tpm.html)
- [QEMU ARM ACPI TPM implementation](https://github.com/qemu/qemu/blob/master/hw/arm/virt-acpi-build.c)
- [Pinned EDK2 QEMU PPI request processor](https://github.com/tianocore/edk2/blob/b03a21a63e3bd001f52c527e5a57feddb53a690b/OvmfPkg/Library/Tcg2PhysicalPresenceLibQemu/DxeTcg2PhysicalPresenceLib.c)
- [swtpm state-encryption and key-FD contract](https://github.com/stefanberger/swtpm/blob/master/man/man8/swtpm.pod)
- [VMware Fusion Apple-silicon feature matrix](https://knowledge.broadcom.com/external/article/315609)
- [VMware Fusion Apple-silicon 3D crash and fallback](https://knowledge.broadcom.com/external/article/426891)
- [Parallels virtual TPM lifecycle](https://kb.parallels.com/en/122702)
- [Parallels DirectX 11/Metal support](https://kb.parallels.com/en/124137)

## Adopted performance policy

BridgeVM has two media-independent launch policies:

- `balanced`: threaded renderer and audited readback behavior; this remains the
  CLI default and the immediate recovery lane.
- `aggressive`: direct renderer, asynchronous scanout, IOSurface GPU blit, zero
  artificial readback interval, and the existing direct-DMA NVMe default. The
  macOS app selects this lane for 3D VMs.

`--performance-risk aggressive` requires `--virtio-gpu-3d`. Every run records
the selected lane and resolved knobs in `preflight.txt`. Switching back to
`balanced` changes no disk, UEFI vars, driver, or TPM state.

We accept correctness risk when all four conditions hold:

1. the risky path has a one-switch rollback;
2. stateful media is not rewritten merely by selecting it;
3. the run emits enough evidence to identify the exact lane;
4. release gates still require a crash-free real-title receipt.

We do not trade away VM identity or recovery-key correctness. TPM state,
Secure Boot variables, and BitLocker-sealed PCR state therefore remain
fail-closed even in `aggressive` mode.

## Security implementation boundary

The local implementation now has three connected layers:

1. BridgeVM exposes TPM 2.0 TIS, PPI 1.3, ACPI `TPM0`, the TPM2 table, a
   relocated 64 KiB event-log allocation, and QEMU's packed 6-byte
   `etc/tpm/config` discovery record. The record lets the pinned EDK2
   `Tcg2PhysicalPresenceLibQemu` initialize policy and process pending PPI
   requests; it is omitted with every other TPM surface when no backend exists.
2. The launcher supervises one swtpm and passes
   `--key fd=0,format=binary,mode=aes-256-cbc`; the upstream contract integrity
   protects the encrypted state with encrypt-then-MAC.
3. The app holds one 256-bit key per stable VM ID in a
   `WhenUnlockedThisDeviceOnly` Keychain generic-password item and supplies it
   through a one-shot pipe. It never writes a keyfile or puts a key in argv.

The failure contract is deliberately strict. New empty state may create a key.
Non-empty state without its Keychain item fails before launch and is left
untouched; BridgeVM does not silently create a replacement identity. Moving a
bundle on the same Mac keeps its ID and key. Cross-Mac migration uses an
AES-GCM-authenticated recovery package whose separate 256-bit recovery code is
never written into that package; restore requires the original VM ID and exact
encrypted-state fingerprint. Cloning allocates a new VM ID and starts with a
fresh TPM without copying the source key. Confirmed reset archives the old
encrypted state and a device-local archive key and writes a lifecycle receipt
before rotating the active identity. BitLocker and other PCR-sealed secrets may
still require guest recovery. VM deletion continues to retain orphaned
Keychain items, favoring recoverability.

The remaining security work is therefore:

1. prove the bundled runtime and authenticated migration lifecycle on a clean
   second Mac with BitLocker enabled;
2. prove `Get-Tpm`, PPI processing, `Confirm-SecureBootUEFI`, PCR 7, populated
   measured-boot events, and recovery-key handling in a fresh same-boot receipt.

No release blocker is cleared by ACPI enumeration alone.
