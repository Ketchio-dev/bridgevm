# V1 handoff — usable Windows-on-Apple-Silicon VM with 3D

## 2026-08-09 — STRATEGY PIVOT: Neptune/Triton graphics stack (commercial)

UTM published Triton (2026-07-24 blog): a real WDDM UMD (DDI) + Neptune
protocol + host D3D renderers, with WHQL-signed KMD releases. Decision made
with the owner: **keep our HVF VMM (QEMU-free = app stays proprietary),
pivot the graphics stack to the Neptune-era components.** License audit in
THIRD-PARTY-NOTICES.md (d9446c4): everything MIT/BSD/Zlib except DXMT
(LGPL, dylib-only if bundled) and D3DMetal (excluded, Apple terms).

Progress tonight (commits 26a762c, d9446c4):
- **usgic boot-park root-caused & fixed 2 layers**: (1) kick storms cleared
  the guest's exclusive monitor every MSI/SGI while the line was already
  asserted -> STXR spinlock livelock (3 vCPUs same PC + cpu0 waiting for
  IPI ack). Fix: kick only on line EDGE (kick_if_line_changed). (2) Apple
  kernel loses vtimer fires under load (258k recovery calls, 25k pulses,
  33k re-arms, no EXIT_VTIMER); usgic now SYNTHESIZES expired fires in
  pre_run from guest CNTV state; vtimer_recovery is IK-only now (its
  re-arm starved the synthesizer's expiry check). Still ~1/3 parks in
  3-boot sample tonight — improved but not closed; re-soak needed.
- **v0.2.1 WHQL KMD (osy/kvm-guest-drivers-windows) works on our VMM**:
  binds DEV_1050 with NO cert/testsigning (Microsoft HW Compat Publisher
  signature). Needs --virtio-gpu-device-id 1050 (10f7 stays for our old
  120.4x images). Desktop GDI works via our local-copy path after fix:
  the new KMD wraps shadow->primary blts in a real virgl ctx
  ("virgl-gdi-blt", RESOURCE_COPY_REGION); submit_3d now tries local
  copies for LIVE contexts too (was pre-context only) -> 254/254 SUBMIT OK
  (b1-kmd-095241).
- **UTM virglrenderer (macos-next) built and deployed**: ~/BridgeVM/3d/
  virglrenderer-utm (build-b2, venus+neptune+vrend; needs no ANGLE — our
  CGL create_gl_context callback supplies GL). Our venus host-pointer
  import patch ported (MTLBuffer import aliasing bug). Old prefix backed
  up at prefix-pre-utm-20260809; vendor tree snapshot committed on branch
  bridgevm-vendor-snapshot-20260809 (a20cbff). Probe links prefix-utm.
- **venus-on-v0.2.1 still unproven**: mesa venus ICD (vulkan_virtio.dll)
  fails before GET_CAPSET. Found+worked around ICD bare-name LoadLibrary
  error 87 (copy dll to System32). Chain requires KMD ADAPTERINFO
  Supports3d (features 0x19 OK) + HasShmem (BAR2 shm cap present) +
  vgpusrv service; verification blocked repeatedly by agent stalls.
- **swap procedure (validated, b2-fresh-113630)**: pnputil /add-driver
  v0.2.1 -> remove-device ALL display nodes -> delete-driver 120.4x oems
  -> scan-devices -> binds 100.6.101.58000. Offline alternative: injector
  built at ~/BridgeVM/injectors/inj-utm021.raw with new
  REMOVE_DRIVER_OEMNAMES support (untested — BootOrder skipped injector).
- **Agent lockstep stalls are the #1 friction**: any guest-side hang of
  the PS agent (GUI-blocking CMD, `cmd /c type` of UTF-16 files, big
  share syncs) wedges the whole CTL pipe (in_flight never times out,
  resident_service.rs). ALSO: re-running a session script truncates
  agent.ctl (`: > CTL`) below the agent's remembered byte offset ->
  appended commands never execute. Workarounds: never `type` UTF-16 to
  console, never run GUI apps blocking, read results via share sync +
  host iconv; new session dir per boot.
- Guest image state: work/b2-fresh.raw = canonical-a2 + v0.2.1 swapped
  (agent flaky after many kills); b1-kmd.raw = older experiment (agent
  dead); canonical-a2-staged UNTOUCHED (its 120.45 INF matches BOTH 10f7
  and 1050 — fresh 1050 boots bind 120.45 until oems deleted).

**LATE-SESSION RESULT — VENUS PROVED on the new stack (b2-fresh-123408):**
PPSSPP --backend=VULKAN produced `ctx 7449 capset 4 venus-win32`:
CREATE -> 2x RESOURCE_CREATE_BLOB -> MAP_BLOB -> 2x SUBMIT_3D (both
OK_NODATA) -> UNMAP -> DETACH -> clean CTX_DESTROY, through the UTM
virglrenderer proxy (vgpusrv=Running; System32 dll copy workaround for
ICD error 87). This already passes the point where the old stack's
2nd-instance wedge lived. PPSSPP itself exited early (likely surface/
swapchain step -> D3D fallback) — next: find why, then bvgpu-real-title
gate on this stack. Image with v0.2.1 swapped saved as
work/canonical-utm021-20260809.raw (+vars; possibly dirty NTFS from
final kill — chkdsk on first boot).

Agent-ops rules learned tonight (CRITICAL for future sessions):
- ctl_offset starts at the file's CURRENT END per boot generation
  (harness_protocol.rs:51) — commands appended BEFORE the probe process
  starts are never seen. Append only after `BVAGENT SERVICE alive`.
- A `cmd /c type <utf-16 file>` or GUI-blocking command wedges the
  agent's single lockstep worker (in_flight never times out).
- Session scripts `: > CTL` truncate resets nothing guest-side; safe
  because offset re-anchors at end-of-file on next generation, but any
  pre-truncate append is lost.

Next steps (in order):
1. venus proof on v0.2.1: fresh clone + swap + reboot; run PPSSPP
   --backend=VULKAN via Start-Process (non-blocking); watch for
   CTX_CREATE context_init=4 and GET_CAPSET(VENUS) in virtio-gpu.jsonl;
   if ICD still fails, get VN_DEBUG=all + VK_LOADER_DEBUG output via
   share (no type); suspects: vgpusrv not running, KMTQAITYPE_
   UMDRIVERPRIVATE query, wire_format_version mismatch vs UTM proxy.
2. If venus proves: 2nd-instance wedge retest (the old A2/A3 killer),
   then A2 gate on this stack.
3. Neptune D3D11 (Triton): install neptune_umd (INF already registers
   UserModeDriverName) — D3D11 apps should go through Neptune ctx to
   dxmt-native; needs dxmt-native build (meson, llvm@15 — Homebrew has
   llvm@20/21 only, check DXMT tolerance or pin llvm@15).
4. usgic re-soak with kick-edge + synthesis (6+ boots, --release,
   3D on); if solid, resume A1 p1 gate.
5. Agent hardening: in_flight timeout + requeue; CTL offset reset
   handling (session nonce line).

## Current state — 2026-08-07 (HEAD `75761f6`, CI+SQ green)

Registry: **19/24 proven**, `ENGINEERING_PREVIEW`.
Open release-blocking: **A1, A2, A3, A11, A15**.

**A14 PROVEN (4181fb2)**: GUI-driven session — packaged app → library VM →
`hvf-runner --launch-spec` → READY on shipping shape (virgl 0x1050, swtpm vTPM
w/ Keychain key) → agent shutdown → SYSTEM_OFF. GUI run found 2 real bugs:
data-protection keychain needs entitlements ad-hoc builds lack (-34018 on
WRITES only — probe must be add+delete, login-keychain fallback added); debug
bundle gate named a deleted Secure Boot policy (bfe5d72). Do NOT redo: the
`win11-typed` library VM at ~/Library/Application Support/BridgeVM/vms/ is the
GUI fixture; computer_task drove Start; cua cannot read the live framebuffer
window (AX timeouts) — verify via logs/hvf/run.log instead.

A1 addendum (2026-08-07): **EL2 measured and closed** (parks before first serial
byte, strictly worse than EL1) AND **wake-relocation closed by inference** (69k
re-entries prove waking is not the gap; delivered interrupts did not save the
boot). **A1's host-side option space is exhausted** — the remaining path is the
Apple Feedback (draft ready, owner files) or a different in-kernel GIC.
A15 best chain: **8 full cycles / 13 generations** at CYCLES=100 before an A1
park (b9fe664). A11 re-run at current head: 653 shim tests pass (2c307c5).
A2 wedge LOCATED (67651d9): 4/6 runs reached READY + full staging, then parked
DURING the title gate — Vulkan startup load triggers the A1 park near-
deterministically. 11 repaired-instrument attempts, 0 fps samples.
A2 wake-rate mitigation falsified (cd4a356): BRIDGEVM_AGENT_WAKE_MS=2000, 3/3
identical parks — cancel RATE is not the term; 14 attempts, 0 samples. Stale
RPR is load-dependent (0x60 virgl / 0x10 venus-headless), added to the
Feedback draft with the full closed-avenue matrix (75761f6).
**Everything open funnels through A1 (owner files the Feedback) or final-head
re-runs (A11; A15 100-cycle is A1-bound; A3 blocked by the same park).**

A14 typed-path slices 12–19 (all proven live or against real processes):
- `vm_process.rs`: env_clear allowlist spawn; EXIT_ON_RESET baked in; env IS
  the manifest (CARGO leak test).
- `vm_run.rs run_vm()`: leases → generations → flush+receipt → decide_restart.
  Live on canonical image: 4 generations, guest resets via agent ctl file,
  three exit-42 recreations, watchdog exit correctly receipt-less.
- `DeviceSurfaces`: ramfb/display/input/venus + clipboard/share/net/audio,
  wrapper env contract byte-for-byte, unit-asserted; per-generation run.log
  append (the app's tail target).
- `vtpm.rs`: supervisor-owned swtpm, one per run across generations;
  key-over-fd-0 encrypted state (wrong key refused, proven vs real swtpm);
  `--helper-vtpm-state` live (tpm2-00.permall durable).
- watchdog_ms: Option — None = app mode = WATCHDOG_DISABLED=1 (absent ≠ 0).
- hvf-runner: `--launch-spec --helper --helper-firmware --helper-agent-control
  --helper-evidence-dir --helper-vtpm-state` all live-proven.

A15: live 3/3 harness + typed-path 4 generations (both documented in
a15-reset-recreate-3cycle-20260806.md). 100-cycle gate A1-bound.

Everything else from the 698c420 state stands (A1 8-soak closure, A2 queue,
A11 recipe, A3 leads, ops notes).

Do NOT redo (evidence in `docs/windows-arm/evidence/`):

- **A3 0xD1 fix** (`1d1457c`), **A19** snapshots, **A16/A17/A18**, **A12/A13**, C1–C16.
- **A11 running**: 651+ XCTest functions RUN via `scripts/run-xctest-shim-suites.sh`.
  OPEN only for re-run at final V1 head.
- **A1 (a1-gic-irq-state-20260806.md, 8 soaks, 14/40)**: every readable GICv3 register
  captured at terminal stalls; every host lever tried AND measured: unmask, CVAL→now,
  CVAL→now+10µs, mask pulse, forced ISPENDR0 (delivers, boot dies — falsified), stale-RPR
  clear (works, delivery still dead), clear-before-pulse ordering. UEFI-shape mechanism:
  swallowed trapped EOI → RPR=0x10 gates everything. Kernel shape: fire never forms
  (ISTATUS=0, CVAL past). Defect isolated inside Apple's in-kernel GICv3/vtimer under
  hv_vcpus_exit pressure; **no host lever remains**. Apple Feedback draft ready:
  `a1-apple-feedback-draft-20260806.md` (owner files it). DO NOT retry latch forging,
  CVAL tricks, or pulse reordering.
- **A2 (a2-a3-title-fps-measurement-20260801.md + 2026-08-06 addenda)**: instrument
  repaired end to end (shared frame-log read, cube.iso content required, --loglevel=3,
  600s per-file share waits). Gate reached REAL-TITLE-PASS with venus_icd_loaded.
  Five runs then wedged in the A1 class before any fps sample. **Formally queued behind
  A1.** net-live-20260724 image unusable (viogpu3d CM_PROB_UNSIGNED_DRIVER); use
  canonical-attach-resident-20260731.
- **A14 slices 1–11** (`f545fd0`→`e2e8c30`): bridgevm-hvf-runtime (manifest, generation,
  receipt, VmEventQueue, VmBuilder flock leases, decide_restart, supervise_reset_cycles);
  hvf-runner --launch-spec (leases live-proven) + --supervise (real processes, 4 cycles
  live); app encodes+materializes launch-manifest.json; probe speaks exit-42
  (--exit-on-reset wrapper flag; env-only failed — wrapper scrubs BRIDGEVM_*).
- **A15 (a15-reset-recreate-3cycle-20260806.md)**: LIVE 3/3 cycles — fresh helper PID,
  increasing generation, fresh BVAGENT READY, 4 CPUs from the guest, real PSCI reset,
  exit-42 asserted. 10-cycle run added intermediate-reset semantics (Windows boots
  reset ~3x; each recreates the process) then ended in the A1 UEFI shape at cycle 3.
  **100-cycle gate is A1-bound, not reset-path-bound.** Harness:
  `scripts/verify-reset-recreate.sh` (CYCLES=100 for the full gate).

Actual next steps:

1. **A1 is the single lever**: file the Apple Feedback (owner); meanwhile the only
   untried host avenue is relocating the guest's boot-critical wake onto an owned
   device interrupt (SPI/MSI) — needs research into which Windows boot phase arms
   the parked WFI wake.
2. **A14 remaining**: VmRuntime::run (probe example still assembles+boots the VM;
   see explore-agent report — lifecycle lives only in the example); then the app off
   the wrapper script.
3. **A3 p50**: DWM composition cost (title pinned at ~4x DWM rate; smoke gets 54 FPS).
   Present-mode experiments (Composed:Flip vs Copy-GPU-GDI) are the lead.
4. On any A1 improvement: rerun t4-soak ×2, then A2 verify, then CYCLES=100 A15.

Ops notes: p1gate-cache prepared-* clones eat ~49 GiB each — keep only the current key;
queue refuses <90 GiB free. GH Actions had a queue-stall day (2026-08-06); rerun
cancelled runs rather than re-pushing.



---

# HVF GIC probe main decomposition handoff

## 1. Goal & status

Goal: decompose `crates/bridgevm-hvf/examples/hvf_gic_boot_probe.rs::main()` without changing runtime behavior, evidence formats, public APIs, fail-closed behavior, or HVF resource lifetime ordering.

Current branch and commit:

```text
refactor/hvf-gic-main-decomposition
12d194d refactor(hvf): decompose GIC probe main
```

Completed:

- The example root shrank from 1,827 to 222 lines.
- The runtime body is in `hvf_gic_boot_probe/probe_runtime.rs` at 998 lines.
- Every newly extracted Rust module is below 1,000 lines.
- `_vm_guard` is constructed at `probe_runtime.rs:34`; `_vcpu_guard` is constructed at line 65. Both remain in `run()`, preserving `_vcpu_guard`-before-`_vm_guard` destruction.
- The `'reboot` loop and labeled control flow remain together in `probe_runtime.rs`: loop at line 115, `continue 'reboot` at 951, `break 'reboot` at 993.
- Structural budgets were lowered to actual sizes and include all new modules.
- `GOAL.md` records completed criteria and evidence.

Verification run after the final code fix:

```text
cargo fmt --all -- --check                                      PASS
cargo check --workspace --all-targets                           PASS, zero warnings
cargo check -p bridgevm-hvf --all-targets --features venus      PASS, zero warnings
scripts/check-refactor-budgets.sh                               PASS
git diff --check                                                PASS
cargo test -p bridgevm-hvf --all-targets                        738 passed, 0 failed, 1 ignored
cargo test -p bridgevm-hvf --all-targets --features venus       740 passed, 0 failed, 1 ignored
review_swarm                                                    clean
```

What does not work / is not complete:

- No known failure remains in this change.
- The repository still has oversized files outside this change: `crates/bridgevm-hvf/src/platform/apple/firmware_run_loop.rs` and `crates/bridgevm-hvf/src/windows_arm/run_loop_render.rs`.
- There is no `.harness.json`; the explicit gates above are the project checks used here.
- After this handoff overwrite, `HANDOFF.md` is modified but not committed.
- `CLAUDE.md` remains an untracked local secret-bearing file and must stay untouched.

## 2. Decisions & why

### Keep the root as a thin explicit-path module host

The root retains imports/module declarations, capability-print handling, exit-code conversion, and a thin call to `probe_runtime::run()`. New files are declared with explicit `#[path = "hvf_gic_boot_probe/..."]` attributes so Cargo does not treat them as independent example targets.

### Keep HVF guards in `probe_runtime::run()`

Setup helpers do not return `HvVmGuard` or `HvVcpuGuard`. Returning guards in an aggregate could change field-based destruction order. VM creation is followed by `_vm_guard`; vCPU creation is followed by `_vcpu_guard`, exactly preserving the original lifetime relationship.

### Do not extract the timing-sensitive CPU0 run loop

The inner execution/exit-dispatch/automation loop remains in `probe_runtime.rs`. The decomposition removed configuration, setup, and terminal reporting around it instead of altering its labeled breaks, continues, lock scopes, interrupt draining, watchdog attribution, or automation ordering.

### Use a macro for terminal reporting

`final_report.rs` defines `persist_and_report_stop!` rather than a function with a very large typed context. This moves the reporting token block while retaining access to local concrete types and preserving the original platform lock interval. Macro metavariables must remain standalone tokens; do not interpolate them into string literals.

### Keep probe-lifetime services outside `'reboot`

Agent service wake, GPU vblank wake, KD serial bridge, and live input remain constructed before the reboot loop. Moving them per-generation would replay input or misattribute stale canceled exits.

### Keep setup responsibilities separate

- `probe_config.rs`: environment/config parsing and startup messages.
- `hvf_setup.rs`: VM and GIC creation.
- `probe_setup.rs`: firmware mapping, TPM backend, and platform construction.
- `boot_media_setup.rs`: vars/media/Linux boot attachment.
- `gpu_shm_setup.rs`: GPU shared-memory mapping port state.
- `watchpoint_setup.rs`: watchpoint environment parsing.
- `final_report.rs`: persistence and terminal diagnostics.

This split gets every file below 1,000 lines without moving the high-risk run loop.

### Replace the prior tracked PLAN.md with the approved plan

The repository already had a tracked 2,493-line `PLAN.md`. The user's explicit instruction was to write the reviewable plan to `PLAN.md`, so it was replaced by the approved 59-line plan and committed in `12d194d`.

## 3. Files modified this session

Committed in `12d194d`:

- `GOAL.md` — goal criteria, status, and evidence.
- `HANDOFF.md` — prior handoff snapshot; overwritten again after the commit by this file.
- `PLAN.md` — approved 59-line implementation plan.
- `crates/bridgevm-hvf/examples/hvf_gic_boot_probe.rs` — thin root wrapper and explicit module declarations.
- `crates/bridgevm-hvf/examples/hvf_gic_boot_probe/boot_media_setup.rs` — boot media and Linux blob attachment.
- `crates/bridgevm-hvf/examples/hvf_gic_boot_probe/final_report.rs` — persistence and terminal report macro.
- `crates/bridgevm-hvf/examples/hvf_gic_boot_probe/gpu_shm_setup.rs` — GPU SHM map-port installation.
- `crates/bridgevm-hvf/examples/hvf_gic_boot_probe/hvf_setup.rs` — VM/GIC creation helpers.
- `crates/bridgevm-hvf/examples/hvf_gic_boot_probe/probe_config.rs` — probe environment configuration.
- `crates/bridgevm-hvf/examples/hvf_gic_boot_probe/probe_runtime.rs` — runtime orchestration, reboot loop, and unchanged CPU0 run loop.
- `crates/bridgevm-hvf/examples/hvf_gic_boot_probe/probe_setup.rs` — firmware/TPM/platform setup.
- `crates/bridgevm-hvf/examples/hvf_gic_boot_probe/watchpoint_setup.rs` — watchpoint configuration.
- `scripts/refactor-budgets.tsv` — lowered root budget and added new module budgets.

Uncommitted after this request:

- `HANDOFF.md` — this updated handoff.
- `CLAUDE.md` — pre-existing untracked local file; not read, modified, staged, tracked, or committed.

## 4. Remaining work

1. Review this updated `HANDOFF.md`, then commit it separately if the handoff should be preserved in git.
2. Confirm `CLAUDE.md` is excluded before any staging command:

   ```bash
   git add HANDOFF.md
   git diff --cached --name-only
   ```

3. Push or open the review for commit `12d194d` plus any handoff-only commit.
4. Start the next structural refactor on a separate branch; first candidate:

   ```text
   crates/bridgevm-hvf/src/platform/apple/firmware_run_loop.rs
   ```

   Do not combine it with this GIC probe change. `crates/bridgevm-hvf/src/windows_arm/run_loop_render.rs` should also be handled separately.

## 5. Failed attempts & dead ends

### Extracting final reporting as a typed function during the first pass

An automated splice inserted the context struct at the wrong syntactic boundary inside reset matching, causing parse errors and many unresolved locals. That attempt was reverted to the last compiling state. Do not repeat an unverified text-boundary splice.

### Naive macro identifier replacement corrupted string literals

The first report macro generation replaced identifier text inside literals, producing strings such as:

```text
PCI boot-$media stats
recent PCI boot-$media requests
$platform mutex
```

`macro_rules!` does not substitute metavariables inside string literals. Review swarm caught the boot-media labels. They were restored to the exact original strings, and all string literals in `final_report.rs` were checked for accidental `$...` text.

### Assuming the first root move was sufficient

Moving all of `main()` into `probe_runtime.rs` made the root small but left `probe_runtime.rs` at 1,641 lines. Additional setup/report extraction was required.

### Setup extraction initially lost `vars_data`

Moving platform construction to `probe_setup.rs` initially left `vars_data` referenced in the caller but not returned. The helper now returns `(VirtPlatform, Vec<u8>, Vec<u8>)` for platform, vars data, and boot DTB.

### GPU SHM helper initially discarded shared state

The first helper returned `()` even though the run loop later locks `hv_gpu_shm_state`. It now returns `Arc<Mutex<HvGpuShmMapState>>` to preserve the original state lifetime and uses.

### Budget ceilings initially omitted unsafe counts

New files were first added with unsafe ceilings of zero, causing `UNSAFE>ceiling` for `hvf_setup.rs`, `probe_runtime.rs`, and `probe_setup.rs`. Their budget entries now record the actual unsafe counts; no existing ceiling was raised.

### Initial review was too broad

A whole-diff review was truncated at 60,000 characters. Follow-up reviews used scoped path lists. The scoped final review reported clean.
