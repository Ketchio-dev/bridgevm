# Resident-agent chunked output and storage-integrity live proof — 2026-07-12

This record closes the unbounded single-frame command-output failure that was
found during WDK diagnosis. It also records a direct-DMA storage baseline and
a separately identified buffered-path comparison. It does not claim that the
Windows 3D package is finalized, bound, or rendering.

## Root cause and protocol change

The guest agent previously encoded the complete output of each command in one
`OUT <exit> <base64>` line. A large diagnostic log therefore exceeded the
practical virtio-console frame boundary, and the guest's `WriteFile` helper did
not retry partial writes. The host kept waiting for a reply while the guest
continued serving heartbeats. Dropping that in-flight request after a timeout
would have misaligned every later response.

The agent now retains the legacy `OUT` form through 24 KiB and uses bounded
`OUTBEG`, ordered `OUTCHUNK`, and `OUTEND` frames above it. The host validates
the declared byte count, chunk count, sequence, per-chunk size, base64 bounds,
and 16 MiB total limit before publishing the existing `CMD`/`END` envelope.
An overdue request remains in flight; a fresh `READY` is the explicit resync
boundary. The guest write helper also retries a partial synchronous write until
the whole frame has been transferred or a real error occurs.

## Live result

- The updated `bvagent.ps1` crossed the shared-folder channel and SHA-256
  matched on the Mac, at `C:\BridgeVMShare\bvagent.ps1`, and after replacement
  at `C:\bvagent.ps1`:
  `79ea8da3a43bd1842c47a80eaff0d30605dd0d4139dcbc845ba66183dff7b17f`.
- A clean Windows restart produced `PSCI SYSTEM_RESET`, `hv_gic_reset = 0x0`,
  and a second resident-agent `READY` in the same VMM process.
- The new agent returned 131,072 bytes generated as
  `0123456789abcdef` repeated 8,192 times. After stripping the command's CRLF,
  the observed length was exactly 131,072 and SHA-256 was
  `a2706a20394e48179a86c71e82c360c2960d3652340f9b9fdb355a42e3ac7691`.
- The immediately following `echo chunk-followup-ok` completed with exit 0,
  proving that the multi-frame reply did not shift the request/reply stream.
- On the normal direct-DMA path, Windows created and force-flushed a 256 MiB
  deterministic file with SHA-256
  `150861404e6616150d2c7d50b90f53ad985c6999f85e2c140636d82f57b6f0a2`.
  After clean PSCI system-off and NVMe writeback, an independent macOS
  read-only NTFS mount reported the same size and hash.
- The run's service gate recorded handshake, initial command completion,
  service start, guest system-off, NVMe writeback, `probe_status=0`, and
  `status=0`.

The wrapper now accepts `--nvme-buffered-io` as an explicit, recorded
diagnostic option. Ambient `BRIDGEVM_*` values remain scrubbed; only the CLI
option adds `BRIDGEVM_NVME_BUFFERED_IO=1`. A separate CoW clone booted with the
probe and preflight both identifying the buffered path, copied the same 256 MiB
file, and returned equal source/destination hashes with exit 0 before the WDK
retry began. That run subsequently installed the WDK and SDK, powered off
cleanly, and matched both 256 MiB files again from an independent host-side
read-only NTFS mount. See
[the complete WDK/SDK and buffered-storage record](wdk-sdk-buffered-storage-20260712.md).

## Preserved evidence index

The completed direct-DMA/reboot/chunk run is preserved at:

```text
/Users/user/BridgeVM/wdk-install-reboot-proof-20260712-v3
```

| File | SHA-256 |
| --- | --- |
| `run.log` | `ac80ffaedab7b18843a08e7e744a4266b1de36ee7caad281a87b9b6a6e3d9373` |
| `agent-service-gate.txt` | `530553e5796aaf1edaff82376487292affc482cca6f8540c72a82295b5dea983` |
| `preflight.txt` | `f1f3e5da760a1832a40d12c44a2f69c8bfa46c0174017cc6d4f9f58ebb773790` |
| `chunked-output-proof.txt` | `f102c28bd818238ccca9263c7495ce94dd2257b1b28a016251bf88540527e326` |
| `direct-dma-storage-proof.txt` | `7f2f15f1b0227f148022ade93c11681c9d0980ff1654091179d4b5928f17b834` |

The completed buffered comparison is preserved at:

```text
/Users/user/BridgeVM/wdk-install-buffered-retry-proof-20260712-v4
```

## Limits

The direct and buffered baselines each prove one deterministic 256 MiB
create/copy/flush/offline-read boundary, not exhaustive storage correctness.
Both passed, so this evidence does not support claiming a direct-DMA corruption.
WDK/SDK success is reported from its own preserved installer logs and must not
be inferred merely from these storage hashes.
