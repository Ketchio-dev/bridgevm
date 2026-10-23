# Live diagnostic records in T17 and T19 lane results — 2026-10-23

Classification: report-only host diagnostics and one documented machine-contract
deviation. The evidence is automated tests (rank 3) and static reasoning (rank
4). **No live run has produced these records or lane fields**, so nothing here
says how a real guest's audio, display or pointer cadence behaves. No gate reads
them. No criterion, threshold, product state, capability wording or reader
bound changes.

Each claim below carries its rank, as in AGENTS.md §1: **[3]** is an automated
test in this repository, **[4]** is static reasoning over the code, and
**[recalled]** is unverified memory of QEMU behaviour. Commits:

- `ec5776f9`: the composite HID deviation;
- `8308638d`: the HDA records in lane results;
- `32f27b94`: the display timing record;
- `8be7511c`: the display record in lane results.

## What a live T17 or T19 lane will now record

The private lane result `lane-N-result.json`, written by BridgeVMProductE2E,
gains one optional key, `first_run_host_records`. The value is read from the
bound post-footer host tail of the run log that the journey archives as
`metadata/product-e2e/first-run.log`. That is the first boot, where the guest
plays audio and takes the keyboard and pointer challenge. **[3]**

The value is one object per record that the host printed, named after the
record's prefix, holding the record's fields as unsigned 64-bit integers:

| Object | Fields |
|---|---|
| `hda_poll` | polls, gap_le_10ms, gap_le_20ms, gap_le_50ms, gap_le_100ms, gap_gt_100ms, max_gap_ms, capped_polls, capped_lost_ms, ioc_completions, max_iocs_per_poll, msis_sent |
| `hda_stream` | runs, rate_hz, channels, bits, cbl_bytes, cbl_us, entries, entry_bytes_min, entry_bytes_max, entry_us_min, entry_us_max, ioc_entries, position_buffer, lpib_reads, running_ms |
| `hda_repeat` | passes, silent_passes, compared_passes, repeat_passes, max_repeat_run |
| `hda_coreaudio_timing` | callbacks, interval_le_10ms, interval_le_15ms, interval_le_20ms, interval_le_30ms, interval_le_50ms, interval_gt_50ms, max_interval_ms, late_callbacks, trylock_misses, ring_samples, ring_min_ms, ring_max_ms, ring_final_ms |
| `ramfb_display_timing` | ticks, late_le_1ms, late_le_4ms, late_le_8ms, late_le_16ms, late_gt_16ms, max_late_us, published, interval_le_20ms, interval_le_40ms, interval_le_70ms, interval_gt_70ms, max_interval_ms |

The HDA records print three-decimal milliseconds (`cbl_ms`, `entry_ms_min`,
`entry_ms_max`). These become integer microseconds under `_us` names. **[3]**

Records are left out in these cases. **[3]**

- A record the host did not print is absent.
- A record with a value beyond u64 is dropped.
- When the host printed none, the key is absent, and the lane result is byte
  for byte the one written before.
- When the log is missing, a symlink, over 64 MiB or without a bound final
  report, the key is absent.

**[3]** Both receipt writers (`write-windows-product-e2e-receipt.py` and
`write-windows-import-product-e2e-receipt.py`) admit the key only in this form:

- known object names;
- exactly those fields;
- `int` values in 0..2⁶⁴−1.

They never copy it into a receipt. A lane without the key is checked exactly as
before.

When each record prints **[4]**:

- The HDA records need an HDA controller whose CoreAudio sink installed its
  host records.
- `ramfb_display_timing` needs a running ramfb display exporter. That is the
  3D-off machine with a BVFB display path.
  - With a virtio-gpu device, the device's sink owns the file and nothing
    prints.
  - With no display path, nothing prints.
  - Both inert cases are tested. **[3]**
- Whether a live T17 or T19 lane's first run meets these conditions has not
  been observed.

## The `ramfb display timing:` record

The export thread counts each tick and each published frame. **[3]**

- **Ticks.** Each tick is counted by lateness, `woke − deadline`, in buckets
  of at most 1, 4, 8 and 16 ms, with the rest in late_gt_16ms. The largest
  lateness is `max_late_us`, in microseconds.
- **Published frames.** Each published frame after the first is counted by
  the interval since the previous publish completed, in buckets of at most 20,
  40 and 70 ms, with the rest in interval_gt_70ms. The longest interval is
  `max_interval_ms`, in whole milliseconds.
- **Sums.** The late buckets sum to `ticks`. The interval buckets sum to
  `published − 1` when anything was published.

How to read the record **[4]**:

- The deadline is the loop's own, and it resynchronises after an overrun.
  Lateness is therefore wake overshoot, not missed frames.
- The exporter publishes only changed frames. An idle stretch with no
  change is one long interval. The interval histogram mixes export cadence
  with how often the guest redraws.

Because the probe joins the exporter before the final report, the thread does
not print the record, and its `stopped` line must precede the footer. Instead,
the exporter leaves a closure with the platform through
`VirtPlatform::set_teardown_host_records`. The platform's last field prints the
record once as the platform drops. It prints after the HDA controller's
records and the CoreAudio sink's lifecycle, stats and continuity records. **[3]**
The lib test pins that order with call-recording closures.

Every reader admits the record there: **[3]**

- the Python twin, `hvf_host_tail.py` with `hvf_host_display.py`;
- the packaged shell twin, `scripts/hvf-terminal-report.sh`;
- the evidence CLI;
- the Swift twin, `HvfHostTail` with `HvfHostDisplay.swift`;
- T17's bounded audio tail.

The readers admit at most one record, after every HDA diagnostic, stats or
continuity record, within the unchanged 16-record and 4,096-byte bounds. They
refuse it before an HDA record, twice, or malformed.

The widest tail is 12 records and 3,680 bytes, with every field at u64::MAX,
three stopping enqueue failures, a failed stop and dispose, and the display
record. **[3]**

What did not change **[3]**:

- **Older logs.** The HDA, continuity, stop-reader, terminal-stop and T17 tail
  contracts run unchanged and pass. The shell's existing order regex is
  byte-identical, and the display placement is a separate check.
- **Record strings.** The export's `ramfb display export: stopped ...` line and
  the HDA records are byte-identical.

**Nothing guest-visible changes.** **[4]**

- The counts use only instants the export thread already reads, on a host
  thread.
- They change nothing the exporter publishes or when.
- They print after the guest has stopped.

So no deviation entry is needed.

## The composite HID deviation

`docs/machine-contract/qemu-virt-deviations-usb-input.json` gains
`xhci-hid-composite-device` (area usb-input, guest_visible). **[3]** This is
schema-validated.

What BridgeVM presents, read from `crates/bridgevm-hvf/src/xhci/usb_descriptors.rs`
and checked by a script kept with the evidence **[4]**:

- **Device.** One high-speed device, bcdUSB 2.00, VID 0x1209, PID 0x0001, with
  no string descriptors besides LANGID 0x0409.
- **Configuration.** One configuration, 100 mA, no remote wakeup, with two
  interfaces:
  - a boot keyboard: endpoint 0x81 (DCI3), 8-byte reports, bInterval 10;
  - an absolute pointer without boot protocol: endpoint 0x82 (DCI5), 6-byte
    reports with 3 buttons, X/Y 0..32767 and a relative wheel.
- **Topology.** It sits on root port 1 of 8 (4 USB 2.0, 4 USB 3.0), in slot 1.

QEMU's `virt` machine with `usb-kbd` and `usb-tablet` instead presents two
separate devices. The deviation entry states this with a confidence per claim.
**[recalled]** It was not checked against QEMU source in this session.

## Not done

- **`xhci hid pacing:` is not implemented.** The design was:
  - per endpoint, DCI3 keyboard and DCI5 pointer: reports sent, a report
    spacing histogram and held reports;
  - overdue pointer wakes, from the probe's pointer deadline check.

  No lane result carries any of it.
- **Its tail budget.** At the widest values the tail has 416 bytes left, and a
  record of that shape would need about 700. **[4]** It must be shorter, or
  print under narrower value widths. No bound may be raised for it.

## Not verified

- **No live run** has printed any of these records. It is unknown what a real
  first run's cadence looks like, or whether the counts fit their buckets
  usefully.
- **The whole lane flow.** The receipt-writer `--check-lane` paths were
  exercised with fixtures **[3]**, not in a live tier.

## Checks

These are local developer checks, not hosted CI. **[3]** The logs are kept
outside the repository with the other evidence of this change.

- **Rust:** `cargo +1.97.0 fmt --check`, both clippy invocations with
  `-D warnings`, `test -p bridgevm-hvf --lib` (1,483 passed, 2 ignored) and the
  probe example (657 passed).
- **rustfmt on the changed example files:** clean. The only hunks are in the
  untouched `ramfb_display_thread_tests.rs`, which was already unformatted at
  base.
- **Swift:** the BridgeVMProductE2E shim suite (419 passed).
- **Python:** the host-tail and lane contracts,
  `windows-product-e2e-contract-smoke.sh` and `check-python-scripts.py`.
- **Budgets:** per commit.
