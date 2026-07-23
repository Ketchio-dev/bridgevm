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
