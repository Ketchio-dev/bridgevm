# BridgeVM Virtual ARM PC: guest PCIe ECAM enumeration (2026-08-30)

## Evidence rank and result

This is a fixed-`N=20` live engineering receipt on real Apple silicon, not a
product release criterion. At exact BridgeVM code head
`afb0106c535863bce542a4ada0b3b589e081baea`, 20 independent signed-runner
processes each created a new HVF VM and executed a minimal EL1 guest at the
BridgeVM Virtual ARM PC v1 RAM base.

Each guest performed exactly eight 32-bit loads from PCIe ECAM base
`0x4000_0000`: the host bridge at `00:00.0`, followed by the seven versioned
board functions at `00:01.0` through `00:07.0`. The host required an
instruction-syndrome-valid read of the expected width, destination register,
IPA and order for every stage-2 data abort before routing it through
`BridgeVmPcPlatform::on_mmio`. Any missing, reordered, unmapped or mismatched
identity failed the lane.

The result was `20/20`, with 160 validated guest ECAM reads and no unexpected
exit. Every lane reported:

```text
BridgeVM Virtual ARM PC PCIe enumeration probe: PASS
board=com.ketchio.bridgevm.virtual-arm-pc abi=1
ecam_base=0x40000000 ecam_size=0x10000000 reads=8
role=host-bridge bdf=00:00.0 identity=0x00081b36
role=system-storage bdf=00:01.0 identity=0x00101b36
role=usb-input bdf=00:02.0 identity=0x000d1b36
role=installer-media bdf=00:03.0 identity=0x10011af4
role=network bdf=00:04.0 identity=0x10411af4
role=display bdf=00:05.0 identity=0x10501af4
role=guest-agent bdf=00:06.0 identity=0x10431af4
role=audio bdf=00:07.0 identity=0x26688086
LIVE PROOF: guest MMIO enumerated all BridgeVM PC v1 PCIe identities
```

## Reproducible identities

The read-only local seal is
`bridgevm-pc-pcie-afb0106c535863bce542a4ada0b3b589e081baea`.

| Item | SHA-256 |
|---|---|
| complete 20-lane log | `8a86c6de8348f42ace448a76f67f6e038153b89b28fb10df4939dc0f699578a6` |
| result record | `15756a69de0a2d1171262a8e2dbdf19425babb666a923c09ff684327394de032` |
| four-file content manifest | `9a1460e1084117e93847ac0384f6b5c626efbbaf1a5327acec81b610d803b525` |
| ad-hoc-signed exact-head runner | `cf3fe610344d9db2570db81fc2aa2aa9f98f052847f1f1358e68cf8a8eb244cf` |
| exact Git source archive | `59c7a707362b001be13e9c014450afd1d2fbd6c27084a1592d0819dc95ff079a` |

The runner carries only the Hypervisor.framework entitlement and is a local
probe, not a redistributable product binary.

The preceding implementation head
`132924f7753e157dc07c10688148810789cdb049` also produced a 20/20 live run,
but hosted CI run `33347271152` correctly failed because the three contract
tests were not yet included in the repository's test-reachability suite. That
result remains retained and does not support this seal. Commit `afb0106` added
the example test target to that suite; all three tests then executed and the
reachability gate reported 332 files and 2,118 tests.

Studio `t0-check` job `20260830-212815-28437-8965` then ran the complete
`scripts/check-project.sh` at `afb0106`. Every code and test step passed,
including all three Swift shim suites; its only failed step was the
intentionally stale capability registry before this documentation seal. The
check log SHA-256 is
`0e26287b12a4bfb24945969efd36afedbf8f25131a1c2c4e30ad508ea5d3120f`.
The preceding job `20260830-212714-28113-14281` failed before tests because the
worker clone had not fetched `afb0106`; it has no receipt and is retained as a
failed submission. GitHub-hosted Security and quality run `33347632521` passed
at the exact code head. GitHub-hosted CI run `33347621986` failed on its first
attempt when the BridgeVMApp shim process hit a Swift runtime retain-count
abort before reporting an assertion result; every other completed job passed.
The same run's exact-head second attempt passed the shim's 419 tests and the
complete hosted matrix. The first attempt remains part of the record.

## Implementation boundary

The new board owns its versioned address map, ACPI MCFG description, PCIe role
ordering and opt-in `BridgeVmPcPlatform` routing. Its host runtime reuses
BridgeVM's existing Rust endpoint models; this receipt does not claim a second
implementation of NVMe, xHCI, virtio or HDA. Sharing a device model does not
make the two guest boards interchangeable: the independent board routes the
same model at its own ECAM and MMIO apertures and records a different board ID
and ABI.

The guest read only vendor/device identity DWORDs. This proves that real HVF
guest MMIO reaches all eight functions at the BridgeVM PC v1 ECAM addresses and
returns the versioned identities. Unit tests additionally bind the seven role
names and BDFs to `bridgevm_pc::PCI_DEVICES` and reject a mismatched result.

## Claim boundary

This does **not** prove firmware `PciBus` discovery, BAR sizing or assignment,
endpoint MMIO, DMA, MSI/MSI-X delivery, NVMe or virtio queues, UEFI Block I/O,
GOP, BDS, Windows installation or Windows boot. It does not change the shipping
Windows board. The independent board remains experimental. A9 remains OPEN,
and the separate B4 result remains PROVEN at 20/20 with p95 243 ms.
