# Native VMM fw_cfg DMA read bound — 2026-10-07

This checkpoint records one deterministic device-model repair. It does not
establish a Windows workload, a physical-HVF run or a release gate. Product
state and all criteria remain owned by `capabilities/windows-hvf.json`.
Integration starts from main `00cb8fd9`.

## Defect

A `fw_cfg` DMA read built the whole transfer in a host buffer before it
checked the guest destination. It read one byte at a time through
`read_data(length)`, so the length was used as the buffer size. The guest
sets that length as a 32-bit field. A single DMA control write that asked for
`u32::MAX` bytes made the VMM allocate and fill 4 GiB of host memory. That took
about 6 s of CPU on the vCPU thread that handled the MMIO exit, and the
transfer then failed because the destination was not guest RAM. A measured
probe of three such requests peaked at 4.3 GB resident.

## Repair

QEMU v11.0.0 (`hw/nvram/fw_cfg.c` `fw_cfg_dma_transfer`) writes guest memory
directly from the entry. Past the entry's end it writes zeros with
`dma_memory_set`, and it stops on the first failed write with
`FW_CFG_DMA_CTL_ERROR`.

BridgeVM now streams the read the same way. It writes the entry bytes that
remain straight from the entry, then zero-fills from a fixed 4 KiB block. The
cursor advances by each chunk, and the first unbacked chunk returns
`DMA_CTL_ERROR`. No host buffer depends on the guest-chosen length. The bytes
written, the cursor and the status match the previous behaviour for every
fully backed transfer. A transfer that is only partly backed now writes its
backed chunks before it reports the error, where it used to write nothing.
QEMU also writes chunk by chunk, so there is no new guest-visible deviation.

The unit tests moved unchanged from `fwcfg.rs` to `fwcfg_tests.rs`, which
lowers the `fwcfg.rs` budget from 771 to 499 lines.

## Discovery and verification

The issue was found by static review of guest-sized DMA buffers, after the
virtio-gpu host-memory finding. It was confirmed by a probe that took 6.2 s per
request at a 4.3 GB peak.

Two new fixtures record each guest write:

- A 64 MiB read aimed at unbacked memory errors after a single 4-byte write.
- A 12 KiB read zero-fills past the 4-byte entry in chunks of at most 4 KiB,
  and writes nothing past the requested length. A read that runs off the end
  of RAM writes the backed prefix and then reports an error.

The original implementation fails both: it issued one 64 MiB write, and one
12 KiB write. The repair passes 21/21 `fwcfg` tests in Debug and Release. The
`bridgevm-hvf` library passes 1336/0 with one existing ignore.

## Boundary

Only DMA reads changed. DMA writes already bounded their staging to 256-byte
chunks and checked the guest range first. Byte-wise `DATA` register reads are
unchanged. No firmware boot or Windows run was performed for this repair.
