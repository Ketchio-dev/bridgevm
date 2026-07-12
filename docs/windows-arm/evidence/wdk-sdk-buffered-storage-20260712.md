# Windows WDK/SDK and buffered-NVMe storage proof — 2026-07-12

This record closes the Windows toolchain-acquisition and buffered-storage
diagnostic gate needed before the pinned `viogpu3d` package can be finalized.
It does not claim that the driver package is signed, installed, bound, or
rendering; those are the next live gates.

## Live result

- An installed Windows 11 ARM64 clone booted with four vCPUs, 6144 MiB RAM,
  userspace NAT, resident BVAGENT control, and the explicitly recorded
  `BRIDGEVM_NVME_BUFFERED_IO=1` diagnostic data path.
- Windows copied the deterministic 256 MiB `C:\BridgeVMStorageA.bin` to
  `C:\BridgeVMStorageB.bin`. Both guest hashes were
  `150861404e6616150d2c7d50b90f53ad985c6999f85e2c140636d82f57b6f0a2`.
- WDK `10.1.28000.1839` installed with bundle exit `0x0`. Its setup recovered
  five transient download failures and 32 stale-cache hash failures by
  reacquiring payloads; 239 acquired payloads were subsequently verified.
- Windows SDK `10.1.28000.2114` installed with bundle exit `0x0`, no restart,
  and a successful exact `winget list` result. Its setup recovered ten
  transient download failures, verified 262 acquired payloads, and reported no
  hash failure.
- The same app first-boot disk-growth command was rerun against the already
  expanded disk and returned exit 0 with
  `BRIDGEVM_DISK_GROW_OK state=already-max size=51249135104
  free=10838659072`.
- `shutdown.exe /p /f` exited 0. The VMM observed PSCI system-off, wrote back
  the 48 GiB NVMe image and writable UEFI vars, and exited 0. The generated
  service gate records `guest_system_off=true`, `nvme_writeback=true`,
  `probe_status=0`, and `status=0`.

## Independent offline verification

After the VMM exited, macOS attached the target RAW image with both media and
NTFS volume read-only. It independently observed both files at 268,435,456
bytes and recomputed the same SHA-256 for A and B. This closes the buffered-path
copy at the post-writeback storage boundary; it is not inferred from the guest
hash output.

The read-only filesystem also contained the exact required tools:

| Supplier | Tool | Installed path |
| --- | --- | --- |
| WDK | InfVerif ARM64 | `C:\Program Files (x86)\Windows Kits\10\Tools\10.0.28000.0\arm64\infverif.exe` |
| WDK | Inf2Cat x86 | `C:\Program Files (x86)\Windows Kits\10\bin\10.0.28000.0\x86\Inf2Cat.exe` |
| Windows SDK | SignTool ARM64 | `C:\Program Files (x86)\Windows Kits\10\bin\10.0.28000.0\arm64\signtool.exe` |

The preserved MSI logs identify those suppliers directly: the WDK
`WindowsToolsVersioned` package installs ARM64/x64 InfVerif, the WDK
`WindowsDriverKitBinariesOnecoreUAP` package installs x86 Inf2Cat, and the SDK
`WindowsSDKSigningTools` package installs ARM64/x64/x86 SignTool. The finalizer
therefore searches both `Windows Kits\10\bin` and `Windows Kits\10\Tools`,
preferring the native architecture when it exists.

## Preserved evidence index

The completed run and extracted installer logs are preserved at:

```text
/Users/user/BridgeVM/wdk-install-buffered-retry-proof-20260712-v4
```

| File | SHA-256 |
| --- | --- |
| `run.log` | `ea66511dca67a1230d5a8720c71ce921896db0c9f95dec18d2106fee96991b72` |
| `agent-service-gate.txt` | `8b5306465f8be9d49cab08f7e58c18320aab2a4fcb8e9d811f929a84efa68ac0` |
| `preflight.txt` | `b0c9959fe95759a182b5c7b84cd01a0596d6b0280fffb6a2787de0ec75f1de13` |
| `buffered-storage-proof.txt` | `bccb2995a84057f506fb801bb0f4bc1bbf4af5c7f84913f84fe874f14a17cb12` |
| `offline-verification.txt` | `2c7d0d8464c094460abeb719ce96a5efbb961d12255c023d6e57c58402edaf8b` |
| `target-stat.txt` | `a6a6482af0b5f3b2823c2d7b6d70eab5111304f64874f3f6fbbd8a8e2efda6a8` |
| `wdk-install-bundle.log` | `ee7447957019490f87407bfe2aae5d3df88b83bebb7eae8ab5ba3609104acd27` |
| `wdk-infverif-msi.log` | `00a02a1a5ea3ad94bcfbecf88d358b4f6c6b43c2899c92331ced6be82647f6a5` |
| `wdk-inf2cat-msi.log` | `da63f08f32dd10f78d5d354dff47b36af157094adc70c1e2c58461cdb831b2ff` |
| `sdk-install-bundle.log` | `9ed0a58467d9cad1c4d5f4a0f272b0e3f77f8a5347e3dceaa2bce66676164fdc` |
| `sdk-signing-tools-msi.log` | `8ad2e1e54bbded35824f8dde9901b41280bb4dea3f4aa57bd4de64dd34722e4b` |

## Audit note and limits

The run log contains expected negative `where` probes for tools in the wrong
`bin`/`Tools` root. It also contains one non-mutating malformed command fragment
(`owsSDK.10.0.28000 ...`, exit 1) caused while appending additional evidence
queries to the live control file. That fragment ran only after the SDK install
had completed successfully and before the correct read-only tool queries; it
did not alter installation, storage, shutdown, or wrapper-gate results.

The earlier direct-DMA run and this buffered run both passed the deterministic
256 MiB boundary. These two passing samples do not establish exhaustive NVMe
correctness and do not justify claiming a direct-DMA corruption. Final package
signing, repository render-candidate validation, test-signing boot policy,
`DEV_1050` PnP bind/Status OK, and a workload-tied VirGL trace remain open.
