# Native VMM PL011 transmit-log bound — 2026-10-07

This checkpoint records one deterministic device-model repair. It does not
establish a Windows workload, a physical-HVF run or a release gate. Product
state and all criteria remain owned by `capabilities/windows-hvf.json`.
Integration starts from main `03566adf`.

## Defect

The PL011 model appended every guest write to `UARTDR` to a host `Vec`. The
product VMM keeps that log for its serial milestone and stop scanners, and
for the stop-time serial file. Only the KD serial bridge drains it, and only
when the operator enables that bridge. Without the bridge, a guest that kept
writing the data register grew host memory by one byte per write, with no
upper limit, for the life of the VM.

## Repair

The transmit log now keeps at most 16 MiB of undrained guest output and
discards later bytes. QEMU's PL011 passes each byte to its character backend
and keeps no backlog, so the guest-visible register behaviour is unchanged:
writes are still accepted, and the flag register still reports an empty
transmit FIFO.

The serial scanners look for boot banners written early in boot, which sit
far below the bound. Draining the log through `take_output`, as the KD serial
bridge does, makes room again.

## Verification

The fixture `pl011_tx_bound_tests.rs` writes 1024 bytes past the bound. It
checks that exactly 16 MiB is retained and that a drain lets the next byte
through. The original implementation retains every byte (16,778,240) and
fails the fixture.

All 8 `pl011` tests pass in Debug and Release, and the `bridgevm-hvf` library
passes 1345/0 with one existing ignore.

## Boundary

Only the retained transmit length changed. Receive injection, register
widths, identification registers and the KD bridge's drain cadence are
unchanged. No firmware boot or Windows run was performed for this repair.
