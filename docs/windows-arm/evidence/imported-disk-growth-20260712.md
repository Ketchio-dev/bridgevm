# Imported Windows disk-growth live proof — 2026-07-12

This record closes the capacity part of the installed-image import path. It is
an audit index for preserved local evidence, not a claim that the custom HVF
engine implements the from-scratch Windows installer.

## Scope

- A cloned 24 GiB installed Windows 11 ARM64 RAW image was extended sparsely to
  48 GiB on the Mac; its source image was not modified.
- The clone and matching writable UEFI vars were booted by the BridgeVM HVF
  engine with 6144 MiB RAM, four vCPUs, and resident BVAGENT control.
- The guest ran `Update-HostStorageCache`, queried the supported C: partition
  size, called `Resize-Partition`, verified free space, and shut down through
  PSCI system-off.

## Observed result

- Windows reported disk size `51,539,607,552` bytes.
- C: grew from `25,478,299,648` to its reported maximum of
  `51,249,135,104` bytes.
- Free space after growth was `26,569,805,824` bytes (24.7 GiB as reported by
  `fsutil`).
- The resize command and `fsutil volume diskfree C:` both exited 0.
- The service gate recorded agent handshake, initial command completion,
  service start, guest system-off, NVMe writeback, `probe_status=0`, and
  `status=0`.
- A post-shutdown host GPT read placed partition 3 at 100,095,967 sectors and
  the secondary GPT table/header at the final LBAs of the 48 GiB image.

The macOS Windows HVF Lab import now applies the same boundary automatically:
it clone/copies the selected source, extends only the imported RAW clone to a
minimum logical size of 64 GiB, records an `hvf-grow-pending` marker, and removes
that marker only after the first live guest resize reports
`BRIDGEVM_DISK_GROW_OK`. Unit tests verify the source bytes remain unchanged,
the imported prefix is identical, the logical size is 64 GiB, and the pending
marker is present. A retry after a successful resize is idempotent: the app
accepts an already-maximized C: only when its partition end is within 16 MiB of
the disk end. A C: partition blocked earlier in the disk still fails closed and
retains the marker.

## Preserved evidence index

The live evidence directory at capture time was:

```text
/Users/user/BridgeVM/wdk-disk-grow-proof-20260712-v1
```

| File | SHA-256 |
| --- | --- |
| `run.log` | `de15f3251f1a44cd26990be400bb7e9eef985fff6b6fc31d70d13bfb448d785f` |
| `agent-service-gate.txt` | `7c975510f8557d78a0e2f5bc5a5d68ec3aebf0a6654062a89d1566ed2589e403` |
| `preflight.txt` | `0898502bc8d6dcf1a5081e7cf3cbecdc20b164d5afe80703cb3d7a84f5f63a71` |
| `target-stat.txt` | `da25d0cc549272474d27c7efc5e012d4f7390c1435d9ae2e4cfabb24719496de` |

## Limits

This proves RAW image extension, live GPT/partition expansion, free-space
recovery, clean shutdown, and the application-owned first-boot workflow. It
does not prove safe automatic growth for an arbitrary partition layout; the
guest command fails closed and retains its marker when Windows cannot extend C:.
It also does not prove from-scratch installation, TPM/Secure Boot, 3D driver
binding, or disk-backed suspend.
