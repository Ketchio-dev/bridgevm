# Windows HVF 20-workload compatibility contract

Status: acceptance contract only; the 20 real payloads and campaign are not yet
present, so this document does not claim compatibility breadth.

## Sealed inputs

A campaign input is a tab-separated file with exactly 20 unique rows and this
fixed header:

```
id executable_sha256 payload_sha256 source version license architecture api category warmup_seconds measurement_seconds
```

(Fields are separated by tabs.) IDs are lowercase stable labels. Every binary
and payload is SHA-256 sealed and carries publisher/source, exact version and
license/provenance. Architectures are `arm64` or `x64`; APIs are `vulkan`,
`d3d11`, `d3d12`, `opengl` or `webgpu`. The measurement window is at least 30
seconds and begins only after the row's declared warmup. Self-authored smoke,
triangle, benchmark and demo fixtures are excluded: they may test plumbing but
must never be relabeled as one of the 20 real workloads. Third-party bytes stay
out of git and CI artifacts.

## Result and raw evidence

The result TSV has the same ordered IDs and fixed columns:

```
id class visible crash_reset clean_shutdown samples p50_ms p95_ms p99_ms series_sha256
```

Classes are exactly `pass`, `degraded`, `unsupported`, `crash`, or
`launch-fail`. A class records what happened to this exact sealed workload; it
is not an API-wide claim. `visible`, `crash_reset`, and `clean_shutdown` are
explicit `yes`/`no` observations.

Each row retains `<id>.frametimes-ms`: one positive finite frame interval in
milliseconds per line, after warmup and in presentation order. The validator
reads the file with `O_NOFOLLOW`, requires a regular file no larger than 8 MB,
and checks its SHA-256 and sample count. It sorts the raw values and computes
nearest-rank indices `floor((N-1)*q)` for q=0.50, 0.95 and 0.99. Reported
p50/p95/p99 must equal those values exactly. Thus scalar results cannot be
invented without retained raw evidence. Missing payloads or evidence fail the
campaign; they are not silently converted to `unsupported`.

The deterministic contract gate is:

```
python3 scripts/validate-windows-compatibility-matrix.py --self-test
```

A real campaign is valid only when the same validator passes all 20 sealed
input/result rows and retained series. Until then the audit row remains OPEN.
