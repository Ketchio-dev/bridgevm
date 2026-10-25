# Who holds the platform lock when the HDA tick is skipped — 2026-10-25

Classification: a report-only host diagnostic and a map of the probe's
platform lock, written on `a0618744`. Evidence ranks follow AGENTS.md:
[rank 2] live single run, [rank 3] automated test, [rank 4] static reading.
**No live run has printed the new record.** Nothing gates on it. No
criterion, threshold, registry entry, capability wording or product state
changes; A5 stays PROVEN and A9 stays OPEN.

## 1. The question

T17 r94's first-ready session skipped 835 of 1199 HDA service-tick attempts
because another thread held the platform lock, and `hda poll:` had five gaps
over 100 ms (the longest 394 ms, 631 ms lost to the cap). The tick itself
woke on time (`max_late_us=2513`), so it was awake and found the lock held
[rank 2]. r95 had no capped gap and a longest gap of 36 ms [rank 2].
`max_busy_run_ms` measures how long a run of busy ticks lasted, not who held
the lock ([withdrawal record](hda-mark-withdrawal-20261024.md)).

## 2. The platform lock

The lock is the `Arc<Mutex<VirtPlatform>>` that
`crates/bridgevm-hvf/examples/hvf_gic_boot_probe/probe_runtime.rs` builds once
per probe process. Every device model lives behind it. What runs under it
[rank 4 unless marked]:

| Site | Thread | Work under the lock | Can it hold for tens to hundreds of ms? |
|---|---|---|---|
| Pre-run drain | each vCPU, before every `hv_vcpu_run` | host time; xHCI drains; virtio-net poll (non-blocking sockets, at most 256 RX frames); virtio-console; GPU fences; the HDA poll (DMA, PCM into the ring); SPI delivery; MSI collection; vCPU0 also pumps the KD serial bridge when one is configured | no: CPU work only |
| Data-abort exit to NVMe | each vCPU | a doorbell write runs `process_into`, which executes every queued submission before the exit returns: whole-transfer `pread`/`pwrite` (MDTS is 0, so a transfer has no size limit), and FLUSH, or a WRITE with FUA or with the volatile cache off, calls `File::sync_data`, which the 1.97.0 std compiles to `fcntl(fd, F_FULLFSYNC)` (read from the binary) | **yes**: host disk latency and full-cache flushes |
| Data-abort exit to the TPM | each vCPU | a STS.go write runs the command as a synchronous swtpm socket round trip (30 s timeouts) | yes, for slow commands such as key creation |
| Other data-abort exits | each vCPU | fw-cfg DMA (boot), legacy virtio-blk ISO reads (install), pflash (in memory), xHCI, HDA and virtio registers | only the boot and install media paths |
| Automation block | vCPU0, when a host wake or due work asks | live input; the agent console, whose resident service reads `agent.ctl`, rescans the share every 3 s (`read_dir` and `symlink_metadata` per file), reads a changed host file for a PUT, and on a completed GET writes a temporary file, calls `sync_all` (also `F_FULLFSYNC`) and renames it; a requested checkpoint (a copy of guest RAM); ramfb sample dumps (opt-in) | **yes**: share I/O with its flush; a checkpoint |
| UART preload | vCPU0, once per boot | queued UART input | no |
| ramfb display exporter | its own thread, `try_lock` | the 28-byte ramfb config | no, and it never waits |
| HDA service tick | its own thread, `try_lock` | `poll_hda_taking_msi` | no, and it never waits [rank 3] |
| Startup, reset, report | main thread | vblank wake state, display checks, the tick's start; a reset under the tick's `quiesce`; the final report after the tick is joined | cannot overlap a running tick (tests count these sites and pin the tick's start, quiesce and join order) |

Most likely long holders, by reading alone: NVMe exits (a flush, or a batch
of queued transfers) and the agent console's share I/O. In T17's audio stage
the guest writes a 192 KB WAV to its disk, plays it for a second and writes
its result file into the share, which the host then fetches and flushes;
r94's first-ready stream ran 6,002 ms in two runs [rank 2], so both kinds of
hold can fall inside a running stream. The TPM matters only when the guest
uses it then. **No evidence attributes r94's gaps to any of them**: its run
log has no per-holder data.

## 3. The `platform lock holds:` record

`crates/bridgevm-hvf/examples/hvf_gic_boot_probe/platform_lock_holds.rs`.
Every other site above that can run while the tick does now takes the lock as
a `HeldPlatform`, naming its site; the tick counts each attempt that finds the
lock held. A hold that such an attempt fell inside is charged, as it ends and
still under the lock, to its holder. The record prints once, from the tick's
thread as it stops, right after `hda service tick: stopped`:

```text
platform lock holds: busy=B unattributed=U <h>_holds=N <h>_busy=K <h>_gt_20ms=L <h>_max_us=M ...
```

for the holders `prerun`, `nvme`, `tpm`, `mmio`, `automation` and `host`, in
that order [rank 3]:

- `busy` counts tick attempts that found the lock held, the same events as
  the stop line's `busy`; `unattributed` is `busy` minus every holder's `busy`;
- per holder: `holds` that a tick found, the tick attempts they blocked
  (`busy`), those holds longer than 20 ms, Windows' whole 20 ms cyclic buffer
  (`gt_20ms`), and the longest in microseconds (`max_us`);
- a data-abort exit is `nvme` when its address decodes to NVMe's BAR0, `tpm`
  in the TPM TIS or PPI region, otherwise `mmio`; `host` is the UART preload
  and the display exporter;
- it reconciles: `busy = unattributed + Σ busy`, `gt_20ms ≤ holds ≤ busy`, and
  `gt_20ms > 0` exactly when `max_us > 20000`.

Cost: a hold reads one atomic and the clock as it starts and the atomic again
as it ends. Only a hold a tick found writes, at most 200 a second with the
5 ms tick. Storage is fixed at 200 bytes and the line is at most 888 bytes
[rank 3]; nothing on the hold path allocates, locks, prints or does I/O
[rank 4]. Holds never overlap, so no attempt is charged twice; an attempt
whose count lands between an acquisition and its first read stays
`unattributed` [rank 4].

The line is outside the bound post-footer host tail, like the stop line: no
reader parses it, and a guest can print a look-alike line elsewhere in the
log, so read the one that follows the tick's stopped line. The guest can
observe none of it, so no deviation entry is needed [rank 4].

## 4. Tests [rank 3]

Nine new tests in `platform_lock_holds_tests.rs` and
`hda_service_tick_holds_tests.rs`: a hold no tick found writes nothing; each
busy attempt is charged once to the hold it fell in; MMIO exits are charged
to NVMe, the TPM or another device by target, including an NVMe address
before its BAR is mapped; the 20 ms boundary and the longest hold; the exact
line; fixed storage and the 888-byte bound; every lock site beside the tick
holds through the record; through the real `service_once` and `tick_once`,
an NVMe exit that keeps the lock across three ticks for over 25 ms is charged
`nvme_holds=1 nvme_busy=3 nvme_gt_20ms=1` with `busy` equal to the stop line's;
and the thread prints the record right after its stopped line. Nine
mutations each fail at least one of them: no busy note, no charge, every MMIO
exit as `mmio`, a charge for every hold, 20 ms counted as long, no print, the
tick not noting busy, the probe's port or the display copy left uncharged.

`lock_platform` moved from `smp_trace/lock_tracing.rs` into the new module
and takes a site instead of a context string; the panic messages are
unchanged. The tick's `sleep_until` and `raise_qos` moved unchanged into
`hda_service_tick_pacing.rs` to keep the tick under its budget.

## 5. What the next live T17 run must show

- A kept run log: a failing lane keeps it, a passing lane deletes it, so a
  passing lane loses this line as it lost r95's `max_busy_run_ms`.
- In the first-run session, `hda service tick: stopped ... busy=B` followed by
  `platform lock holds: busy=B` with the same `B`, and a small
  `unattributed`.
- Against `hda poll:`'s `max_gap_ms=G` and `capped_polls=C`: a holder whose
  `max_us` is close to `G`×1000 and whose `gt_20ms` covers `C` held the lock
  through the long gaps. NVMe would point at moving its I/O off the platform
  lock, a guest-visible timing change that needs a deviation entry;
  `automation` at the agent console's share I/O.
- If every `max_us` stays far below `G`, no single hold made the gap; compare
  the tick's `max_late_us` and `ramfb display timing:`'s `max_late_us` for a
  host-wide stall. Many short `prerun` or `mmio` holds with a large `busy`
  mean contention, not a long holder.

## 6. Not verified

- No live run has printed the record; which holder caused r94's gaps is
  still unknown.
- Getting the record into a passing lane's `first_run_host_records` would
  need the bound post-footer tail and every reader of it; that tail has
  416 bytes left at its widest and this line takes up to 888, so it would
  need a shorter form.
- The latency hold after a capped gap stays open; closing it without
  dropping PCM still needs a sample-rate converter or a lossless re-prime.
