# Goal

## Goal

Decompose `hvf_gic_boot_probe.rs::main()` into responsibility-focused modules without changing behavior, evidence formats, public APIs, fail-closed behavior, or HVF resource lifetime ordering.

## Done-criteria

- [x] `hvf_gic_boot_probe.rs` and every new Rust module are below 1,000 lines.
- [x] `_vm_guard` is constructed before `_vcpu_guard`, both remain in the runtime caller, and `_vcpu_guard` therefore drops first.
- [x] The `'reboot` loop and its labeled `continue`/`break` remain in the same runtime function.
- [x] Formatting, workspace checks, venus checks, structural budgets, and diff whitespace checks pass with zero warnings/errors.
- [x] Default bridgevm-hvf tests report exactly 738 passed and venus tests report exactly 740 passed.
- [x] Review swarm reports no confirmed issues, and `CLAUDE.md` is neither staged nor committed.

## Status

- [x] File-size decomposition proven.
- [x] HVF guard ordering proven.
- [x] Reboot control-flow ownership proven.
- [x] Static verification gates proven.
- [x] Exact test counts proven.
- [x] Review and secret-file exclusion proven.

## Evidence log

- File-size decomposition → `wc -l` reports root 222 lines; new modules 13–998 lines (`probe_runtime.rs` is 998).
- HVF guard ordering → `probe_runtime.rs:34` constructs `_vm_guard`; `probe_runtime.rs:65` constructs `_vcpu_guard`, both in `run()`.
- Reboot control-flow ownership → `probe_runtime.rs:115` owns `'reboot`; labeled continue/break remain at lines 951 and 993.
- Static verification gates → `cargo fmt --all -- --check`, `cargo check --workspace --all-targets`, and venus check all exited 0; `scripts/check-refactor-budgets.sh` printed `PASS`; `git diff --check` printed `PASS`.
- Exact test counts → default test output: `738 passed; 0 failed; 1 ignored`; venus output: `740 passed; 0 failed; 1 ignored`.
- Review and secret-file exclusion → scoped `review_swarm` reported clean; `git diff --cached --name-only` and `git ls-files` confirm `CLAUDE.md` is neither staged nor tracked.
