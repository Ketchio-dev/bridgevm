# Performance Tests

Future boot, resume, idle CPU, display, and disk I/O tests live here.

Current smoke coverage is limited to metadata and bounded host-side artifacts.
These outputs are suitable for CLI/socket/API and dashboard card/status wiring,
but they are not live VM performance evidence.

The current baseline command is intentionally metadata-only:

```bash
bridgevm performance baseline <vm> --output <dir>
```

It does not execute benchmarks, start or resume the VM, generate disk or display
load, or collect fresh host telemetry. It snapshots the performance context
BridgeVM already has: VM state, runner metadata, guest-tools runtime state,
guest metrics when present, derived metadata-only measurement records, and notes
about missing inputs.

Each run writes:

```text
<output>/bridgevm-performance-<vm>-<timestamp>/performance-baseline.json
```

Use this artifact as the first shared baseline for both Fast Mode and
Compatibility Mode. Later real performance tests should attach measured boot,
resume, idle CPU, display, and disk I/O results to the same metadata context
instead of replacing it.

The `measurements` array contains observations computed only from existing
metadata, such as runner observed uptime, guest CPU percent, guest memory used,
and the age of the stored metrics. These records are not benchmark results.

The first execution-backed host-side sample command is:

```bash
bridgevm performance sample <vm> --output <dir> [--artifact-bytes BYTES] [--iterations N] [--sync]
```

This is separate from `performance baseline`: the baseline remains
metadata-only, while `performance sample` is allowed to create bounded host-side
probe files. It still avoids guest workloads, VM boot/resume, display timing,
and guest disk I/O benchmarks. It writes a bounded `write-probe.bin` file for
one iteration, or numbered `write-probe-0001.bin` files for repeated samples,
into:

```text
<output>/bridgevm-performance-sample-<vm>-<timestamp>/
```

and records per-iteration write results plus aggregate measurements such as
`host_artifact_write_latency_microseconds`,
`host_artifact_write_latency_min_microseconds`,
`host_artifact_write_latency_max_microseconds`,
`host_artifact_write_latency_mean_microseconds`,
`host_artifact_write_latency_p50_microseconds`,
`host_artifact_write_total_bytes`, and
`sample_generation_duration_microseconds` in `performance-sample.json`. It also
records BridgeVM metadata operation latencies:
`bridgevm_state_read_latency_microseconds`,
`bridgevm_runner_metadata_read_latency_microseconds`, and
`bridgevm_guest_tools_status_inspect_latency_microseconds`. When available after
a successful Compatibility Mode disk inspection, samples may also include
`disk_inspect_duration_microseconds`. That field measures the host-side
inspection call duration only; it is not a guest disk I/O benchmark, disk
throughput measurement, or replacement for future disk I/O tests. The probe
files are kept so the sample artifact shows what was written.

Safety caps keep this command suitable for routine smoke checks: the default
probe size is 1 MiB, default iterations is 1, per-iteration probe size is capped
at 64 MiB, and total probe output is capped at 256 MiB. `--iterations 0` is
rejected. `--sync` includes host probe-file `sync_data()` in each iteration's
measured write latency, so synced and unsynced samples should be compared as
different measurement modes.

Use sample artifacts to make the next performance work visible without
pretending they are full benchmarks: compare metadata read/status latency,
host-artifact write latency, optional disk inspection duration when present,
total bytes, iteration count, and total sample generation duration alongside the
metadata-only baseline before adding boot, resume, display, idle CPU, or guest
disk I/O tests.

Dashboard performance cards should surface the same artifact path, generation
time, byte-count, iteration, and latency metadata. They must not present either
baseline or sample output as proof that a VM booted, resumed, rendered frames,
or completed guest disk/CPU benchmarks.
