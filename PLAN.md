# PLAN: Decompose `hvf_gic_boot_probe.rs::main()`

## 1. Goal & done-criteria

Behavior-preserving decomposition of the 1,639-line `main()` (lines 189–1827) in `crates/bridgevm-hvf/examples/hvf_gic_boot_probe.rs`. Done when:

- `hvf_gic_boot_probe.rs` and every new module are each < 1,000 lines.
- No changes to runtime behavior, evidence formats, public APIs, or fail-closed behavior.
- `_vcpu_guard` drops before `_vm_guard` (both constructed in the caller, order unchanged).
- `break 'reboot` / `continue 'reboot` remain in the function owning the `'reboot` loop.
- All verification gates pass with exact test counts (738 default / 740 venus), zero warnings.
- `CLAUDE.md` never read, staged, or committed.

## 2. Files to touch

- `crates/bridgevm-hvf/examples/hvf_gic_boot_probe.rs`
  - Extract VM/GIC/platform/media/RAM setup (~lines 239–587) into helper(s) returning raw
    setup values; keep `HvVmGuard` (line 265) and `HvVcpuGuard` (line 417) construction
    at their current logical points in the caller.
  - Extract terminal persistence + stop diagnostics (~lines 1435–1817) into a
    context-based helper (`persist_and_report_stop(&mut VirtPlatform, &StopReportContext)`);
    `break 'reboot` stays in the caller.
  - Move the decomposed probe-body function into a new module; leave a thin `main()`
    wrapper plus imports/module declarations (~188 lines) at the root.
- `crates/bridgevm-hvf/examples/hvf_gic_boot_probe/probe_runtime.rs` (new)
  - Probe-body function, setup helpers, `StopReportContext`, reporting helper. Declared
    from the root with explicit `#[path = "hvf_gic_boot_probe/probe_runtime.rs"]` so no
    stray example target is created. Split further if it approaches 1,000 lines.
- `scripts/refactor-budgets.tsv`
  - Lower the root's ceiling to its actual new size; add new module(s) at actual sizes.
    Raise no existing ceiling.

## 3. Verification

Run after each coherent extraction (`cargo check`) and all of these before commit:

```bash
cargo fmt --all
cargo check --workspace --all-targets                          # zero warnings
cargo check -p bridgevm-hvf --all-targets --features venus     # zero warnings
scripts/check-refactor-budgets.sh
git diff --check
cargo test -p bridgevm-hvf --all-targets                       # exactly 738 passed
cargo test -p bridgevm-hvf --all-targets --features venus      # exactly 740 passed
```

Then `review_swarm` must come back clean. Commit via `git add -A ':!CLAUDE.md'`; verify `CLAUDE.md` unstaged.

## 4. Risks / unknowns

- Setup region (~239–587) is inside `unsafe {}` with two guard constructions interleaved;
  helper boundaries must not reorder guard creation or wrap guards in a returned struct
  (drop order hazard documented in HANDOFF §5). Compiler output is ground truth for types.
- Reporting block mutates the platform for persistence; missing a mutation in the context
  extraction would silently change evidence output. Diff serial/report output paths carefully.
- Line-count math is estimated; if root + wrapper still exceeds 1,000 lines, a third
  extraction (e.g. reset/watchdog decision block) is needed — scope may grow.
- Do NOT touch the timing-sensitive run loop (~lines 790–1361) this pass.
- Exact test counts could shift if `#[cfg(test)]` scoping is disturbed; any count change is a failure.
