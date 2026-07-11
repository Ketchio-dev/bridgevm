# BridgeVM HVF VMM — Performance / Policy Optimization Campaign

Risk-ordered, independently-shippable stages. The from-scratch HVF VMM boots a
networked multi-core Windows 11 to the desktop. The platform still has a single
global platform Mutex, but the release/daily path now has a clean, valid
smp=1/2/4 performance floor. The campaign uses that evidence to decide whether
more invasive locking work is justified before tuning defaults.

Debug `smp=1` remains the default, proven fallback throughout. Every wrapper/env
change is opt-in; every behavioural change has a kill switch; `cargo test -p
bridgevm-hvf` must stay green at every stage boundary.

## Measurement discipline (all stages)
- 3 runs per config, report median. Record before/after in the commit message.
- **Never measure with `BRIDGEVM_SMP_TRACE=1`** — its try_lock + 1ms-sleep loop
  (`examples/hvf_gic_boot_probe.rs` ~L460-497) fabricates lock latency.
- Use `scripts/report-hvf-boot-timer-metrics.sh <evidence-dir>...` after the
  three-run matrix; it reads each `run.log` plus `preflight.txt` and emits
  per-run BOOT_TIMER rows and config-group medians.
- Use `scripts/run-hvf-boot-timer-matrix.sh --target <raw> --vars <fd>
  --evidence-dir <fresh-dir> --release -- --daily --watchdog-ms 120000
  --virtio-net --enable-xhci --shutdown-after-agent-ready` to run the default
  smp=1/2/4, 3-run BOOT_TIMER matrix with per-run cloned media and an automatic
  median report. Runs are interleaved by repetition (1→2→4, three times) rather
  than grouped by vCPU count, reducing order/thermal bias. The default desktop
  oracle is the injected Windows logon agent's READY/PONG handshake; unlike an
  exact whole-frame checksum it is not changed by clock or notification pixels.
  `--shutdown-after-agent-ready` sends the fixed `shutdown.exe /p /f` command
  from the periodic host-wake path, so it does not reintroduce an every-exit
  automation lock, and requires both the agent handshake and PSCI SYSTEM_OFF.
  A failed shutdown gate makes the wrapper/report row invalid; report labels
  include `shutdown` and `console_periodic` so incompatible evidence is not
  silently combined. The legacy
  `--boot-timer-desktop-checksum64 <checksum>` oracle remains available for
  agent-free images. Stale run/report paths are rejected, and
  failed/incomplete/non-desktop runs are marked invalid instead of entering the
  medians. It uses APFS clonefile by default and fails rather than full-copying
  large raw disks unless `--copy-media` is explicit.
- The installed-boot wrapper exposes this as `--boot-timer`,
  `--boot-timer-ramfb-ms`, `--boot-timer-desktop-agent`, and the legacy
  `--boot-timer-desktop-checksum64`, so smp=1/2/4 comparisons can be run without
  manually exporting probe env vars.
- **Re-sign after every probe rebuild** (`codesign --sign - --entitlements
  apps/macos/HvfRunner.entitlements --force <bin>`) or `hv_vm_create` returns
  0xfae94007 — looks like breakage, isn't.
- On faster builds, fixed input fire-delays / ramfb checkpoints calibrated to
  debug speed land on the wrong screen — prefer serial-marker-triggered inputs.

## PERF track
- **Stage 0 — boot-timing harness** (enabler): implemented in
  `hvf_gic_boot_probe` behind `BRIDGEVM_BOOT_TIMER=1`. It emits UART milestone
  timestamps using incremental serial scans, samples ramfb/virtio-gpu
  `checksum64` at `BRIDGEVM_BOOT_TIMER_RAMFB_MS` intervals, reports
  `desktop_reached` when the requested agent oracle connects or the optional
  exact `BRIDGEVM_BOOT_TIMER_DESKTOP_CHECKSUM64` matches, and
  prints exits/sec per vCPU at shutdown. The installed-boot wrapper supplies the
  separate clean-shutdown gate described above. Keep the probe instrumentation
  SMALL (a prior profiling attempt wedged the probe).
- **Stage 1 — release build** (biggest low-risk candidate): implemented. Root
  `Cargo.toml` `[profile.release]`
  has opt-level=3, lto="thin", **codegen-units=1**, **overflow-checks=true**,
  debug=1, and panic stays unwind. Windows/Linux boot wrappers accept
  `--release` while defaulting to debug; P3 GPU mode implies release unless
  `--debug-build`. The 2026-07-11 release/daily matrix at 6144 MiB used the
  agent oracle, periodic shutdown command, round-robin order, fresh APFS clones,
  and produced 9/9 valid READY+SYSTEM_OFF runs:

  | vCPUs | Median desktop READY | Median full lifecycle | Valid |
  | ---: | ---: | ---: | ---: |
  | 1 | 40.372 s | 64.112 s | 3/3 |
  | 2 | 31.193 s | 52.044 s | 3/3 |
  | 4 | 26.137 s | 56.100 s | 3/3 |

  Desktop READY is the performance decision metric; full lifecycle includes a
  variable Windows shutdown interval. Four vCPUs improve READY by 35.3% versus
  one and 16.2% versus two. The report is preserved at
  `/Users/user/BridgeVM/perf-matrix-final-periodic-round-robin-20260711-v1/boot-timer-report.tsv`.
  A historical manual observation suggested roughly 90s→45s but predates this
  validity contract and is not used as campaign evidence. Cheap experiment
  still pending: does DRIVER_PNP_WATCHDOG
  (0x1D5) dissolve at 5-30x faster emulation? If so it MASKS, not root-causes —
  keep the debug repro.
- **Stage 2 — per-exit overhead hygiene** (LOW risk, trace/diagnostic paths):
  2a lazy PCIe ECAM serial phase formatting is implemented by snapshotting the
  bounded UART tail and escaping only when printed; 2b `recent_pcie_mmio` stores
  Copy fields and formats only in print(); 2c `nvme_trace_enabled()` is cached in
  OnceLock; 2d incremental serial scanning is implemented for stop scans and the
  boot timer, and the default automation block is gated so it only takes the
  platform mutex after UART output changes (opt-in automation still preserves
  the old every-exit check); 2e overlay read-merge range start is implemented
  and covered by a partial-read regression test; 2f virtio-gpu trace JSONL
  recording reuses retained line/field buffers and writes event names plus
  common/queue/fence/command detail fields directly into those buffers. Do NOT
  touch `record_command_trace`.
- **Stage 3 — DMA path** (HIGH impact, MEDIUM risk): code implemented and the
  final live matrix covers its current-path correctness; isolated before/after
  performance attribution remains pending. The
  guest-memory trait has default `read_into(&mut [u8])`, live RAM overrides it,
  NVMe SQE/PRP/list paths use caller-owned buffers, the NVMe backend has
  `read_at_into` plus coalesced buffered/direct host-pointer read/write paths,
  PRP span decode and segment coalescing reuse controller-owned scratch vectors,
  xHCI TRB/control/context reads use `read_into`, virtio descriptor metadata
  reads use caller-owned buffers, and guest-controlled variable-length
  blk/net/console/gpu payloads are range-checked and bounded before reusable
  scratch grows (virtio-blk media reads are chunked at 64 KiB). Firmware/stage1
  fixed-size reads use caller-owned buffers, fw_cfg DMA writes chunk through scratch buffers,
  the legacy synthetic virtio-blk write path passes guest slices directly, and
  NVMe Identify/Get Log Page/APST feature responses use fixed stack/const
  buffers instead of per-command heap `Vec`s, and NVMe doorbell completion-event
  collection appends into platform-owned scratch. Virtio-gpu immediate
  2D/cursor/3D queue commands reuse descriptor/request/response scratch buffers,
  and 3D capset payloads fill the caller-owned response buffer instead of
  cloning a backend `Vec`; virtio-net TX reuses
  descriptor/packet scratch buffers, virtio-net RX writes the net header and
  frame slices directly into the guest descriptor chain without first building a
  contiguous packet scratch buffer, and backend receive frames drain into
  device-owned RX scratch instead of forcing a fresh device-side frame buffer;
  NAT ARP replies build the final Ethernet/ARP
  frame directly, DHCP replies build the final Ethernet/IP/UDP/DHCP frame
  directly, gateway/raw-socket ICMP replies build the final Ethernet/IP/ICMP
  frame directly, host-socket UDP/TCP replies build final Ethernet/IP transport
  frames directly, host-socket TCP polling reuses removal scratch, virtio-blk ISO
  reads reuse descriptor and media-read scratch buffers, raw ISO reads zero-fill
  only EOF padding/tails instead of pre-zeroing every read buffer, virtio-console
  control/agent TX queues reuse descriptor/read scratch buffers, control-RX
  messages use inline fixed buffers instead of per-message heap `Vec`s, agent RX
  writes host-to-guest `VecDeque` split slices straight into posted descriptors
  without a staging copy, the live agent-console harness drains guest-to-host
  inbound bytes into retained scratch without taking and reallocating the device
  inbound buffer; validated request gathering stays bounded while 2D/guest
  blob scanout pixel reads no longer allocate per pixel. 2D
  `RESOURCE_ATTACH_BACKING` now validates the complete
  request first and then rewrites the existing resource backing buffer in-place
  instead of allocating a temporary backing vector. Guest-backed virtio-gpu blob
  scanout row compositing reuses device-owned row scratch on repeated flushes;
  blob creation reuses host-iovec scratch through the 3D layer and the Venus FFI
  `iovec` conversion instead of allocating conversion buffers per blob.
  Virtio-gpu reset now clears and resizes the retained scanout buffer instead of
  allocating a replacement framebuffer.
  Virtio-gpu fence completion now drains backend completed fences through a
  caller-owned backend trait buffer into reusable scratch, removes ready fenced
  responses in-place, and recycles delivered descriptor/response buffers back
  into queue scratch or a small parked-buffer pool while preserving both
  completion and pending buffers; Venus
  fence polling walks the retained live-context list directly instead of
  collecting context ids each poll, and the resident agent-console service loop
  snapshots only small in-flight request metadata so large `SharePut` payloads
  are not cloned while processing replies. The same
  live service path now reuses its framed-line output buffer, host-to-guest
  service request/chunked PUT line buffer, CLIPSET CRLF scratch, and guest-path
  scratch while base64-encoding directly into the outgoing line; it also avoids
  the per-line intermediate byte `Vec` while splitting guest replies, and
  chunked GET replies handle unlabelled payload chunks before cloning the
  command label; control-file tailing reuses byte/line scratch buffers and the
  share scanner avoids cloning the root path on each scan; shared-folder path
  normalization now builds the normalized host/guest form in one pass instead
  of `replace` + `collect` + `join`; shared-folder listing reconciliation reuses
  its temporary present/file maps across scans; host directory scans fill a
  retained `HostFile` scratch vector instead of allocating scan results each
  pass, build relative paths without a `collect`+`join` intermediate, and feed
  already-normalized names into the sync engine without normalizing them again;
  LS/LSR reply parsing fills a retained guest-listing scratch vector instead of
  allocating a fresh `Vec` for each `LSOK`, feeds already-normalized guest
  names into the sync engine, and stores only file stat data in its scratch
  listing map; shared-folder PUT replies avoid cloning file metadata on chunk
  ACKs and terminal `PUTOK`/`ERR` responses.
  RAMFB snapshots now fill the retained frame buffer directly,
  BOOT_TIMER virtio-gpu samples summarize
  borrowed scanout bytes instead of cloning the full framebuffer, and opt-in
  RAMFB/input checkpoints defer virtio-gpu scanout access until an actual
  checkpoint is emitted, then summarize and write borrowed bytes instead of
  cloning the full framebuffer. Virtio-gpu blob scanout metadata now uses a
  borrowed blob-resource view so repeated guest-backed flushes do not clone the
  blob backing entries.
  Host-socket NAT polling reuses UDP/TCP/ICMP receive scratch buffers instead of
  reinitializing them on every poll/flow.
  Late fixed-size trace reads
  are covered too: xHCI TRB/context trace, PE owner trace, Arm64 instruction
  trace, WFI summary windows, and the debug watchpoint read now use `read_into`.
  Remaining `read_bytes` uses are trait implementations, test helpers, dynamic
  guest-byte dump helpers, or offline synthetic probe/readback checks rather than
  known live DMA hot paths.
- **Stage 4 — tighten the single platform lock** (attacks the smp=2 regression;
  order 4a,4d,4b,4c): 4a atomic pending-work gate on the secondary pre-run drain
  is implemented in `hvf_gic_boot_probe`
  (`BRIDGEVM_DRAIN_GATE=0` kill switch) — idle secondaries taking the global lock
  per timer/WFI exit is the likeliest cause of smp=2 being slower; 4b
  `hv_gic_send_msi` delivery is now outside the platform lock while level SPI
  remains inside; the pending IRQ queue `mem::take` capacity loss is fixed, and
  live MSI-X/SPI drains now reuse caller-owned scratch buffers instead of
  allocating returned `Vec`s on every non-empty drain. MSI-X pending drains walk
  the pending bitset rather than every advertised vector, so empty/sparse drains
  do less lock-held work, and xHCI caches interrupter pending/enabled bitsets so
  status checks and sparse MSI-X flushes avoid scanning all interrupters; device
  MSI-X flushes append directly into the platform pending queue instead of
  building intermediate message vectors, with virtio-net, virtio-console, and
  virtio-gpu tracking pending queues by bitset so network/agent/display IRQ
  flushes skip their queue arrays when no queue is pending; NVMe submission
  processing now tracks SQ tail-doorbell work by bitset so speculative queue
  drains skip the full SQ vector when no submission queue has advanced;
  4d resolves each platform MMIO target once and caches the most recent BAR;
  4c moves platform-independent probe recorders out of the data-abort lock scope;
  4d skips the redundant setup-input drain after `on_mmio` already ran it.
  **STOP RULE reached:** the valid matrix has smp=2 and smp=4 desktop READY
  below smp=1, so Stage 5 is not justified by current evidence; proceed to
  POLICY.
- **Stage 5 — finer-grained locking** (ENV-GATED `BRIDGEVM_FINE_LOCKING=1`,
  highest correctness risk, DEFERRED, only if future profiling shows residual
  lock-wait): 5a
  RwLock read fast path for NVMe/xHCI BAR0 reads (both `mmio_read` are `&self`;
  liveness breadcrumbs → atomics); 5b per-device Mutex split only after 5a
  profiling shows cross-device concurrent MMIO. Heaviest live matrix; gate-off
  must be byte-identical.

## POLICY track (after Stages 0-4 establish the floor)
- **P1 — daily env profile** (wrapper-only): `BRIDGEVM_RAM_MIB=6144` (8192 on
  ≥32 GiB hosts), `BRIDGEVM_BOOT_PROBE_WATCHDOG_MS=86400000`,
  `BRIDGEVM_NVME_DISK_WRITABLE=1` for persistent disks (default COW overlay grows
  unbounded), keep a host-side image backup; `--daily` now also pins
  `BRIDGEVM_XHCI_REPORT_INTERVAL_MS=30` so measurement runs record the pacing
  policy explicitly.
- **P2 — SMP default + NVMe interrupt spread** (small code, after Stage 4):
  NVMe interrupt spread is implemented: `NVME_MSIX_VECTOR_COUNT` is 9
  (`src/pcie.rs`) so Windows can spread storage completions across vCPUs, and
  `MAX_QUEUE_ENTRIES` is 1024 with queue-create validation for requests beyond
  advertised `CAP.MQES` (prep for async IO). `--daily` already opts into
  `BRIDGEVM_SMP_CPUS=4`, and the installed-boot wrapper now has
  `--smp-cpus` for controlled smp=1/2/4 live boot comparisons. Making 4
  vCPUs the non-daily default is now a policy/UX decision rather than a missing
  measurement; the default debug path was not part of this release matrix and
  keeps its explicit smp=1 fallback.
- **P3 — 1080p display** (firmware, not VMM code): rebuild vendored ArmVirtQemu
  GOP at 1920x1080 or persist via vars flash; host ramfb handles any geometry.
  ~4.3x pixel traffic on unaccelerated CPU drawing — do last.

## Backlog (out of campaign scope)
- NVMe worker-thread async IO (doorbell → worker + MSI-X completion) — the real
  IO unlock, but a threading/ordering redesign; after Stage 5.
- `panic = "abort"` — REJECTED (breaks the probe's join().expect() propagation).
- Guest kernel-debug of DRIVER_PNP_WATCHDOG if the Stage 1 experiment doesn't
  dissolve it.

## Top 3 campaign risks
1. Stage 5 finer locking regressing correctness (changed IRQ timing vs the proven
   Windows NVMe-boot/input evidences — DRIVER_PNP_WATCHDOG class). Env-gate, last,
   stop rule, heaviest live matrix.
2. Wall-clock-calibrated automation drift on faster builds — use serial-marker
   triggers + Stage 0 milestones as ground truth.
3. Silent traps: forgetting `overflow-checks=true`; the unsigned-binary
   0xfae94007 after rebuild; measuring with `BRIDGEVM_SMP_TRACE=1`.

_Derived from a 4-survey + synthesis workflow (build profile, per-MMIO-exit
overhead, SMP lock contention, policy knobs), 2026-07-05._
