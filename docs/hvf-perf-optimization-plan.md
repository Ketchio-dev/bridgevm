# BridgeVM HVF VMM — Performance / Policy Optimization Campaign

Risk-ordered, independently-shippable stages. The from-scratch HVF VMM boots a
networked multi-core Windows 11 to the desktop, but every live boot to date used
the **debug (opt-level=0) build** with a **single global platform Mutex**, so it
is far slower than it needs to be. This campaign establishes the performance
floor (PERF track) before tuning defaults (POLICY track).

Debug `smp=1` remains the default, proven fallback throughout. Every wrapper/env
change is opt-in; every behavioural change has a kill switch; `cargo test -p
bridgevm-hvf` must stay green at every stage boundary.

## Measurement discipline (all stages)
- 3 runs per config, report median. Record before/after in the commit message.
- **Never measure with `BRIDGEVM_SMP_TRACE=1`** — its try_lock + 1ms-sleep loop
  (`examples/hvf_gic_boot_probe.rs` ~L460-497) fabricates lock latency.
- **Re-sign after every probe rebuild** (`codesign --sign - --entitlements
  apps/macos/HvfRunner.entitlements --force <bin>`) or `hv_vm_create` returns
  0xfae94007 — looks like breakage, isn't.
- On faster builds, fixed input fire-delays / ramfb checkpoints calibrated to
  debug speed land on the wrong screen — prefer serial-marker-triggered inputs.

## PERF track
- **Stage 0 — boot-timing harness** (enabler, do first): one env-gated
  (`BRIDGEVM_BOOT_TIMER=1`) module (~100-150 lines, no threads, no locks) that
  emits milestone timestamps (scan only new UART bytes), a ramfb-hash
  desktop-reached detector, and exits/sec per vCPU at shutdown. Keep it SMALL (a
  prior profiling attempt wedged the probe).
- **Stage 1 — release build** (biggest win, near-zero risk; measured ~90s→~45s
  boot-to-desktop, ~2x). Root `Cargo.toml` `[profile.release]`: opt-level=3,
  lto="thin", **codegen-units=1** (the big win on this 86k-line zero-dep crate),
  **overflow-checks=true** (keep debug-proven arithmetic — do NOT drop), debug=1,
  panic stays unwind. Wire `--release` through the boot wrappers (default stays
  debug). Cheap experiment: does DRIVER_PNP_WATCHDOG (0x1D5) dissolve at 5-30x
  faster emulation? If so it MASKS, not root-causes — keep the debug repro.
- **Stage 2 — per-exit overhead hygiene** (LOW risk, trace/diagnostic paths):
  2a lazy `serial_phase_from_uart`; 2b `recent_pcie_mmio` stores Copy fields,
  formats only in print(); 2c cache `nvme_trace_enabled()` in OnceLock; 2d
  **incremental serial scanning + gate the automation block** (O(serial_len)→O(1)
  per exit, helps smp=1 too — likely the biggest late-boot lock-held cost); 2e
  overlay read-merge range start. Do NOT touch `record_command_trace`.
- **Stage 3 — DMA path** (HIGH impact, MEDIUM risk): add `read_into(&mut [u8])`
  (default-impl'd) to the guest-memory trait; use it on NVMe SQE/PRP/data, virtio
  descriptor, xHCI TRB reads. NVMe backend: per-controller scratch buffer,
  coalesce adjacent offsets into single read_at/write_at (128KB read today = ~32
  allocs + 32 syscalls + 2 copies). Land trait change and coalescing as separate
  commits; new coalescing/overlay tests.
- **Stage 4 — tighten the single platform lock** (attacks the smp=2 regression;
  order 4a,4d,4b,4c): 4a atomic pending-work gate on the pre-run drain
  (`BRIDGEVM_DRAIN_GATE=0` kill switch) — idle secondaries taking the global lock
  per timer/WFI exit is the likeliest cause of smp=2 being slower; 4b move
  `hv_gic_send_msi` out of the lock (MSI edge-triggered; keep level SPI inside);
  4c move probe recorders out of the data-abort lock scope; 4d resolve
  `mmio_target` once, MRU cache, skip empty second drain, fix `mem::take`
  capacity loss. **STOP RULE: if smp=2 ≤ smp=1 wall time after Stage 4, Stage 5 is
  not justified — go to POLICY.**
- **Stage 5 — finer-grained locking** (ENV-GATED `BRIDGEVM_FINE_LOCKING=1`,
  highest correctness risk, LAST, only if Stage 4 shows residual lock-wait): 5a
  RwLock read fast path for NVMe/xHCI BAR0 reads (both `mmio_read` are `&self`;
  liveness breadcrumbs → atomics); 5b per-device Mutex split only after 5a
  profiling shows cross-device concurrent MMIO. Heaviest live matrix; gate-off
  must be byte-identical.

## POLICY track (after Stages 0-4 establish the floor)
- **P1 — daily env profile** (wrapper-only): `BRIDGEVM_RAM_MIB=6144` (8192 on
  ≥32 GiB hosts), `BRIDGEVM_BOOT_PROBE_WATCHDOG_MS=86400000`,
  `BRIDGEVM_NVME_DISK_WRITABLE=1` for persistent disks (default COW overlay grows
  unbounded), keep a host-side image backup; keep `BRIDGEVM_XHCI_REPORT_INTERVAL_MS=30`.
- **P2 — SMP default + NVMe interrupt spread** (small code, after Stage 4):
  default `BRIDGEVM_SMP_CPUS=4`; `NVME_MSIX_VECTOR_COUNT` 2→9 (`src/pcie.rs:174`)
  so Windows spreads storage interrupts across vCPUs; `MAX_QUEUE_ENTRIES` 64→1024
  (prep for async IO).
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
