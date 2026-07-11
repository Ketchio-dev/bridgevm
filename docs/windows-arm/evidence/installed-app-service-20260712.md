# Installed Windows app-service live proof — 2026-07-12

This record describes one preserved, mutable-media live run of the BridgeVM
Hypervisor.framework VMM. It is an audit index, not a substitute for the
preserved evidence files.

## Scope

- No QEMU or Apple Virtualization.framework backend was used.
- The input was a cloned 24 GiB installed Windows 11 ARM64 RAW disk plus its
  matching cloned 64 MiB writable UEFI variables file.
- The release `hvf_gic_boot_probe` ran with 6144 MiB RAM, four vCPUs,
  virtio-net, a persistent BVAGENT service control file, and a host/guest share.
- The source disk and source vars identity records were byte-for-byte identical
  before and after the run:
  `16777229:71079998 size=25769803776 blocks=36504032 mtime=1783777849` and
  `16777229:71079999 size=67108864 blocks=131072 mtime=1783777859`.

## Observed result

- `BVAGENT READY` arrived at 27,131 ms.
- The initial `whoami` command completed with exit code 0 and an `END` marker.
- `BVAGENT SERVICE start` arrived at 27,642 ms.
- A 36-byte `host-proof.txt` crossed host-to-guest; a later guest `type` command
  returned `bridgevm-app-service-proof-20260712` with exit code 0.
- `ver` returned `Microsoft Windows [Version 10.0.26200.8037]` with exit code 0.
- `shutdown.exe /p /f` completed with exit code 0, followed by PSCI system-off.
- The VMM reported 190 successful NVMe writes and 18 successful flushes, then
  wrote back the UEFI vars and target disk.
- `agent-service-gate.txt` recorded every boolean as `true`, `probe_status=0`,
  and `status=0`; cleanup recorded no surviving owned process or tmux session.

## Preserved evidence index

The local evidence directory at capture time was:

```text
/Users/user/BridgeVM/app-service-cli-proof-20260712-v1
```

Key file SHA-256 values:

| File | SHA-256 |
| --- | --- |
| `run.log` | `90eda92fdb8fc34eceeebcbb19437f2b0d40141f6fde5f79753ba297d339fd90` |
| `agent-service-gate.txt` | `785b0c8b89ff768093e72fef12bc1dfa82e5702649bf2919622a0dd72a88b7f8` |
| `preflight.txt` | `c7cdb49f2e81b827065ae0c4d15ea71239dbd5734177f9c99a3811b3b82e2253` |
| `target-stat.txt` | `53424709a6355fd5a23417bd208892739079202a1ee2fd77e81666aba69468ef` |

## Limits

This proves an already-installed image can boot, accept service commands and a
shared-file round trip, shut down cleanly, and persist writes through the custom
HVF VMM. It does not prove a from-scratch Windows installer flow, TPM 2.0,
Secure Boot, a distributable WDDM/3D stack, or disk-backed suspend/resume.
