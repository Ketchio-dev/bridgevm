# HVF structural refactor handoff

## 1. Goal & status

Goal: finish the structural-refactor work listed in `docs/refactor-handoff.md` without changing runtime behavior, evidence formats, public APIs, or fail-closed behavior, and bring every Rust file below 1,000 lines.

### Completed and committed

Commit `3c3e0bc` (`refactor(hvf): split GIC probe support by responsibility`) on parent branch `refactor/hvf-gic-boot-probe-modules` completed the pure item/method move for:

- `crates/bridgevm-hvf/examples/hvf_gic_boot_probe.rs`
- `crates/bridgevm-hvf/examples/hvf_gic_boot_probe/agent_console.rs`

Results:

- `agent_console.rs` shrank from 3,232 lines to 51 lines.
- Agent-console state, scripted protocol, resident services, clipboard handling, shared-folder handling, configuration, service wake, and tests are in responsibility-named modules.
- Probe support code is in responsibility modules for HVF ABI, guest memory, SMP/vCPU coordination, exception tracing, environment parsing, reboot/watchdog policy, vCPU debugging, guest diagnostics, storage reporting, boot telemetry, interrupt delivery, WFI diagnostics, and host support.
- Every extracted module is below 1,000 lines.
- `scripts/refactor/roundtrip.py` accepts individual `.rs` crate-root files as well as directories.
- `scripts/refactor-budgets.tsv` includes the probe example tree.
- `docs/refactor-handoff.md` records the corrected classification of the root file.

Verification passed before commit:

- `cargo fmt --all`
- `cargo check --workspace --all-targets` with zero warnings
- `cargo check -p bridgevm-hvf --all-targets --features venus` with zero warnings
- `scripts/check-refactor-budgets.sh`
- `git diff --check`
- `cargo test -p bridgevm-hvf --all-targets`: exactly 738 passed
- `cargo test -p bridgevm-hvf --all-targets --features venus`: exactly 740 passed
- review swarm: clean

### Current state

Current branch:

```text
refactor/hvf-gic-main-decomposition
```

It was branched directly from commit `3c3e0bc`. No source changes have been made on this branch yet.

Working tree:

```text
?? CLAUDE.md
?? HANDOFF.md
```

`CLAUDE.md` is a local secret-bearing user file. It has not been read, modified, staged, quoted, or committed.

What still does not meet the standard:

- `crates/bridgevm-hvf/examples/hvf_gic_boot_probe.rs` is 1,827 lines.
- Its `main()` is 1,639 lines, currently lines 189-1827.
- Pure item movement cannot reduce it further because `main()` is one indivisible item. Behavior-preserving helper extraction is required.
- `crates/bridgevm-hvf/src/platform/apple/firmware_run_loop.rs` and `crates/bridgevm-hvf/src/windows_arm/run_loop_render.rs` remain oversized and untouched.

There is no `.harness.json` in the repository, so no harness `check` command is available. Use the verification gates documented above.

## 2. Decisions & why

### Structural moves and function decomposition stay separate

The committed change contains only item/method relocation plus the minimum internal visibility changes needed for sibling modules. The `main()` decomposition is on a new branch because function extraction changes function bodies and must not be mixed with pure moves.

### The original handoff classification was corrected

The original handoff said `hvf_gic_boot_probe.rs` was solvable by moving items. Parser inventory proved that `main()` alone is 1,639 lines and contains no nested movable functions, modules, structs, or macros. The document now explicitly says the root needs separate function decomposition.

### Do not extract the timing-sensitive run loop first

Keep the CPU0 execution/exit-dispatch and automation path in its current order during the first decomposition pass. Current high-risk region is approximately lines 790-1361. It contains labeled-loop `break`/`continue` behavior, interrupt draining, watchdog handling, timer exits, HVC/PSCI handling, automation, input injection, and framebuffer sampling.

### Preserve HVF guard drop order

Current setup creates `_vm_guard` before `_vcpu_guard`, so Rust drops `_vcpu_guard` first and `_vm_guard` second. Do not return both from a helper in a struct unless field/drop order is explicitly proven equivalent. The safer plan is:

1. extract setup phases that return raw setup values;
2. construct/retain `_vm_guard` and `_vcpu_guard` in the caller at the same logical points;
3. only aggregate non-guard runtime state.

### A root wrapper is likely required

The crate root has roughly 188 lines of imports/module declarations before `main()`. Even reducing `main()` to approximately 900 lines would leave the file over 1,000 lines. After helper extraction, move the resulting probe-body function into an explicitly pathed responsibility module and keep a tiny crate-root `main()` wrapper.

This must preserve the example-root path rule: every new root sibling module needs an explicit path such as:

```rust
#[path = "hvf_gic_boot_probe/probe_runtime.rs"]
mod probe_runtime;
```

Do not place generated `.rs` files directly beside the example root without explicit path handling; every `.rs` directly under `examples/` becomes a Cargo example target.

### Budget treatment

The committed budget records the root at its real current size, 1,827 lines. Lower it only after the decomposition actually shrinks the file. Do not raise any existing ceiling.

## 3. Files modified this session

Committed in `3c3e0bc`:

- `crates/bridgevm-hvf/examples/hvf_gic_boot_probe.rs`
- `crates/bridgevm-hvf/examples/hvf_gic_boot_probe/agent_console.rs`
- `crates/bridgevm-hvf/examples/hvf_gic_boot_probe/agent_console/clipboard.rs`
- `crates/bridgevm-hvf/examples/hvf_gic_boot_probe/agent_console/config.rs`
- `crates/bridgevm-hvf/examples/hvf_gic_boot_probe/agent_console/control_file.rs`
- `crates/bridgevm-hvf/examples/hvf_gic_boot_probe/agent_console/harness_protocol.rs`
- `crates/bridgevm-hvf/examples/hvf_gic_boot_probe/agent_console/protocol.rs`
- `crates/bridgevm-hvf/examples/hvf_gic_boot_probe/agent_console/resident_service.rs`
- `crates/bridgevm-hvf/examples/hvf_gic_boot_probe/agent_console/service_wake.rs`
- `crates/bridgevm-hvf/examples/hvf_gic_boot_probe/agent_console/share.rs`
- `crates/bridgevm-hvf/examples/hvf_gic_boot_probe/agent_console/state.rs`
- `crates/bridgevm-hvf/examples/hvf_gic_boot_probe/agent_console_tests.rs`
- `crates/bridgevm-hvf/examples/hvf_gic_boot_probe/boot_telemetry.rs`
- `crates/bridgevm-hvf/examples/hvf_gic_boot_probe/exception_trace.rs`
- `crates/bridgevm-hvf/examples/hvf_gic_boot_probe/guest_diagnostics.rs`
- `crates/bridgevm-hvf/examples/hvf_gic_boot_probe/guest_memory.rs`
- `crates/bridgevm-hvf/examples/hvf_gic_boot_probe/host_support.rs`
- `crates/bridgevm-hvf/examples/hvf_gic_boot_probe/hvf_abi.rs`
- `crates/bridgevm-hvf/examples/hvf_gic_boot_probe/interrupt_delivery.rs`
- `crates/bridgevm-hvf/examples/hvf_gic_boot_probe/probe_env.rs`
- `crates/bridgevm-hvf/examples/hvf_gic_boot_probe/reboot_watchdog.rs`
- `crates/bridgevm-hvf/examples/hvf_gic_boot_probe/secondary_vcpu.rs`
- `crates/bridgevm-hvf/examples/hvf_gic_boot_probe/smp_trace.rs`
- `crates/bridgevm-hvf/examples/hvf_gic_boot_probe/storage_reporting.rs`
- `crates/bridgevm-hvf/examples/hvf_gic_boot_probe/vcpu_coordination.rs`
- `crates/bridgevm-hvf/examples/hvf_gic_boot_probe/vcpu_debug.rs`
- `crates/bridgevm-hvf/examples/hvf_gic_boot_probe/wfi_diagnostics.rs`
- `docs/refactor-handoff.md`
- `scripts/refactor-budgets.tsv`
- `scripts/refactor/roundtrip.py`
- `scripts/refactor/split_hvf_gic_boot_probe.py`

Uncommitted now:

- `HANDOFF.md` — this file only.
- `CLAUDE.md` — pre-existing local untracked secret file; untouched.

## 4. Remaining work

### Next concrete step: decompose `hvf_gic_boot_probe.rs::main`

Start with non-run-loop regions and compile after each coherent extraction.

Recommended sequence:

1. **Inventory exact current setup boundaries and types again before editing.** The committed root starts `main()` at line 189. Use the compiler as ground truth for returned state types.
2. **Extract VM/GIC/platform/media/RAM setup**, but keep `_vm_guard` and `_vcpu_guard` construction in the caller to preserve destruction order. Current setup region is approximately lines 239-587.
3. **Extract terminal persistence and diagnostic reporting** from approximately lines 1435-1817 into a context-based helper. Keep `break 'reboot` in the caller because a helper cannot target the caller's loop label.
4. **Move the decomposed probe-body function into an explicitly pathed module** such as `hvf_gic_boot_probe/probe_runtime.rs`, leaving a small root `main()` wrapper. Ensure both files are below 1,000 lines.
5. Regenerate `scripts/refactor-budgets.tsv` downward for the root and add any new modules at their actual sizes.
6. Run all required verification gates and exact test counts.
7. Run `review_swarm` before declaring the change complete.
8. Commit with `git add -A ':!CLAUDE.md'`, then verify `CLAUDE.md` is not staged.

Suggested reporting-helper shape:

```rust
fn persist_and_report_stop(
    platform: &mut VirtPlatform,
    context: &StopReportContext<'_>,
)
```

The context needs references/copies for media, vCPU, guest RAM, stop reason/code, exit counters, boot timer, secondary exit counts, drain stats, MMIO/PCI/XHCI traces, trigger lists, redistributor range, and final PCs. The reporting block does not reassign caller bindings, but it mutates the platform referent for persistence.

Do not move `break 'reboot` or `continue 'reboot` into helpers.

After this root is compliant, handle each remaining function in a separate branch/change:

1. `crates/bridgevm-hvf/src/platform/apple/firmware_run_loop.rs`
2. `crates/bridgevm-hvf/src/windows_arm/run_loop_render.rs`

Required gates for every stage:

```bash
cargo fmt --all
cargo check --workspace --all-targets
cargo check -p bridgevm-hvf --all-targets --features venus
scripts/check-refactor-budgets.sh
git diff --check
cargo test -p bridgevm-hvf --all-targets
cargo test -p bridgevm-hvf --all-targets --features venus
```

Expected counts:

- default: exactly 738 passed
- `venus`: exactly 740 passed

## 5. Failed attempts & dead ends

### Treating the example root as a directory in `roundtrip.py`

Initial command:

```bash
python3 scripts/refactor/roundtrip.py crates/bridgevm-hvf/examples/hvf_gic_boot_probe.rs
```

failed with `NotADirectoryError` because the tool only accepted directories. This was fixed in commit `3c3e0bc`; do not add temporary-directory workarounds.

### Assuming item moves could make the root compliant

Parser inventory showed `main()` itself is 1,639 lines. No combination of top-level item moves can make the root smaller than that. Do not retry a pure regroup of the root.

### Assuming two helper extractions alone make the file smaller than 1,000 lines

An exploration estimate subtracted setup and reporting from the 1,639-line `main()` and estimated a ~907-line result. That ignores the crate root's ~188 lines of imports/module declarations. The whole file would still be roughly 1,095 lines. A tiny root wrapper or an additional extraction is required.

### Returning both HVF guards from a setup helper without proving drop order

This was identified as unsafe structurally, not implemented. A returned struct may change guard destruction order depending on field order. Keep guard creation/lifetime explicit in the caller unless an exact equivalent drop order is demonstrated.

### Removing `use super::*` from the extracted agent-console tests

The non-test example build reported the import unused when the tests module lacked `#[cfg(test)]`; removing it broke test compilation with many unresolved private support names. The correct fix was to put `#[cfg(test)]` on the containing `agent_console_tests` module and keep `use super::*` inside the test module.

### Moving agent-console tests one directory deeper without adjusting `include_str!`

Placing tests at `agent_console/tests.rs` broke the relative path to `scripts/win-assets/bvagent.ps1`. The tests were instead placed at `hvf_gic_boot_probe/agent_console_tests.rs`, preserving the original relative `include_str!` path. Do not move that test file deeper unless all relative compile-time paths are deliberately updated and verified.
