# Native VMM virtio PCI ISR decode panic — 2026-10-07

This checkpoint records one deterministic device-model repair. It does not
establish a Windows workload, a physical-HVF run or a release gate. Product
state and all criteria remain owned by `capabilities/windows-hvf.json`.
Integration starts from main `b5855d63`.

## Defect

The modern virtio-net, virtio-gpu and virtio-console PCI functions advertise
their ISR capability at BAR4 offset `0x1000`. Each transport first tested the
common-config window and then the device-config window. The device-config test
was a range check followed by `offset - 0x2000`, and Rust evaluates
`then_some`'s argument eagerly. Every access below `0x2000` that missed the
common window therefore subtracted with overflow. That covers every ISR
access.

Release builds keep overflow checks, so the first guest read or acknowledge of
the ISR register panicked the VMM thread. Guests that use MSI-X never touch the
ISR, which is why the Windows paths ran without hitting it. A driver that falls
back to INTx, or that polls the ISR, would crash the VMM. Virtio-blk decodes
the ISR offset before any window, so it was not affected.

## Repair

The common and device window decoders now use `checked_sub` followed by a
size filter. Window membership is unchanged, and an out-of-window offset now
falls through to the ISR and notify decoders as originally intended.

## Discovery and verification

The defect was found by a temporary, uncommitted harness that issued random
guest MMIO to a fully populated `VirtPlatform` with random guest RAM. It
panicked on its first seed. After the repair, the harness ran 20 seeds of
5,000 accesses and then a longer background run, with no panic.

Two new fixtures use guest PCI configuration and BAR accesses only:

- ISR read and acknowledge on virtio-net and virtio-console.
- A virtio-gpu resolution change, observed through the ISR config-change bit
  and then acknowledged.

The original implementation failed both fixtures with
`attempt to subtract with overflow` in Debug and Release (0 PASS / 2 FAIL).
The repair passes 2/0 in both. The `bridgevm-hvf` library passes 1334/0 with
one existing ignore in Debug and Release.

## Boundary

This covers only the BAR4 window decode. It does not establish INTx delivery
for these functions on Windows, legacy virtio-pci transports, or any guest
workload. No guest-visible deviation is added: QEMU completes ISR accesses
normally.
