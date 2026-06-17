# Performance Tests

Future boot, resume, idle CPU, display, and disk I/O tests live here.

Current safe smoke coverage is limited to metadata and bounded host-side
artifacts. Socket requests can attach a bounded in-guest benchmark only when the
daemon already owns a running backend with a connected benchmark-capable
guest-tools session; local/offline samples remain host-only.

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
metadata-only, while local/offline `performance sample` is allowed to create
bounded host-side probe files. It still avoids VM boot/resume and display
timing. It writes a bounded `write-probe.bin` file for one iteration, or
numbered `write-probe-0001.bin` files for repeated samples, into:

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

When the socket request is handled by a daemon-owned running backend with an
authenticated guest-tools session advertising `benchmark`, the daemon also sends
a bounded `RunBenchmark` command and appends `guest_benchmark_*` measurements to
the same sample artifact. The refreshed guest-tools runtime metadata records the
latest benchmark command result payload, so the artifact carries both the host
probe and the guest-side result without inventing a separate transport.

Safety caps keep this command suitable for routine smoke checks: the default
probe size is 1 MiB, default iterations is 1, per-iteration probe size is capped
at 64 MiB, and total probe output is capped at 256 MiB. The guest benchmark
duration is capped by the guest-tools protocol. `--iterations 0` is rejected.
`--sync` includes host probe-file `sync_data()` in each iteration's measured
write latency, so synced and unsynced samples should be compared as different
measurement modes.

Use sample artifacts to make the next performance work visible without
pretending they are full benchmarks: compare metadata read/status latency,
host-artifact write latency, optional disk inspection duration when present,
total bytes, iteration count, optional daemon-owned guest benchmark values, and
total sample generation duration alongside the metadata-only baseline before
adding boot, resume, display, idle CPU, or broader guest disk I/O tests.

Dashboard performance cards should surface the same artifact path, generation
time, byte-count, iteration, host latency metadata, and optional
`guest_benchmark_*` measurements. They must not present either baseline or a
host-only sample as proof that a VM booted, resumed, rendered frames, or
completed guest workloads.
