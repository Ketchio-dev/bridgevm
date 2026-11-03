# AGENTS.md — how we work here

BridgeVM is a Windows-on-Mac virtual machine app. The goal is a
Parallels-class experience that beats QEMU-based VMs on measured numbers. Ship
small working improvements quickly, measure them, keep what wins.

These rules replace the earlier gate-heavy process (operator decision,
2026-11-02). When a rule here and a habit of the old process disagree, this
file wins.

## 1. Autonomy

Work without asking for approval: pick the most valuable next thing,
implement it, test what matters, open a PR, merge it when its required
checks are green, and fix or revert a red `main` before anything else.
Before stopping, leave the state and the next step in the wiki (section 8).

Ask the operator only for what an agent cannot or must not do: spending
money or creating accounts; passwords, keys and certificates; physical
presence (console unlock, reboot, OS install); macOS privacy or security
settings (TCC); legal or licensing calls; publishing a release; touching the
operator's own data, images or VMs.

## 2. Keep it simple — the main rule

- Build the real user path first. Handle failures that actually happen,
  not every failure you can imagine.
- Validate at trust boundaries only: bytes from the guest, file paths, the
  network, anything security-relevant. Inside our own code, trust the
  invariants; a `debug_assert!` beats a new error type.
- Be tolerant with our own components: ignore unknown fields, optional
  markers and harmless formatting differences instead of failing the whole
  operation.
- A refusal is a bug unless it protects user data or security. When one
  feature fails, switch that feature off, keep the VM running, and tell the
  user in one plain sentence what happened and what to do.
- Do not add gates, manifests, registries, budgets or other process unless a
  real incident shows it is needed. Delete checks that never catch bugs.
- Tests cover real behaviour and real regressions, not exhaustive matrices
  of hypothetical input. One focused test that fails before the fix beats
  twenty that cannot fail.
- Prefer deleting code to adding it. Equal results with less code wins.

## 3. Numbers decide performance

- The perf harness (`vm_bench.py`) boots one Windows clone under BridgeVM and
  QEMU/HVF and reports boot time, idle CPU, memory, CPU, memory
  bandwidth, disk, flush and process-spawn numbers. Run it before and after
  a performance change; keep the change only if it wins beyond run-to-run
  noise without making data less safe.
- Say how you know: measured on a VM, tested, or reasoned. Never present a
  guess as a measurement, and never move a target to make it pass.

## 4. Honest product claims

- README, STATUS and app text describe what actually works. User-facing
  capability wording comes from `capabilities/windows-hvf.json`; its
  `product_state` changes only with the operator.
- Never advertise a security feature that is not implemented. Guest-visible
  randomness comes from the host CSPRNG.

## 5. Data and secrets

- Never commit or upload Windows media paths, disk images, UEFI vars, vTPM
  state, keys, passwords or other secrets.
- Never boot or modify canonical images or the operator's VMs directly. Test
  runs use `cp -c` APFS clones with their own vars file and delete the
  clones (and any vTPM key they created) afterwards.
- Guest flushes are ordered by default: acknowledged after `F_BARRIERFSYNC`
  and made durable with `F_FULLFSYNC` within about a second, so a host power
  loss can drop the last second of writes but never leaves the guest disk
  inconsistent. A per-VM setting makes every flush durable before it is
  acknowledged, and a clean guest shutdown always ends with `F_FULLFSYNC`
  (operator delegation, 2026-11-03).

## 6. Pull requests and CI

- Small PRs. Before pushing, run the focused tests for what you changed, and
  `scripts/check-project.sh --fast` when you touch build or CI plumbing.
- Merge with a merge commit once the required checks pass on the exact head.
  Other workflows are signal: if one turns `main` red, fix or revert first.
- The commit message and PR body are the record: what changed, why, how it
  was tested, numbers if any. No seal commits, no per-change history docs.
- A release candidate runs the full `scripts/check-project.sh`; the release
  workflow also requires a fresh `tested_commit` (`BRIDGEVM_RELEASE_GATE=1`).
- CI runs on GitHub-hosted runners only; never attach a self-hosted runner to
  this public repository. VM runs that need a real Mac happen locally or on
  the physical-Mac live queue, never in public CI.

## 7. Practical conventions

- Source `../../env.sh` before building when it exists.
- Files under `scripts/win-assets/**` keep CRLF line endings; normalise guest
  logs with `tr '\r' '\n'` before parsing.
- Guest agent: append to `agent.ctl` only after `BVAGENT SERVICE start`, keep
  shared files under about 8 MB, wait for results by file name.
- Long jobs run in the background; check back instead of blocking a session.
- `GOAL.md`, `PLAN.md` and `HANDOFF.md` belong to the operator; never stage
  them. Remove temporary instrumentation before you finish.

## 8. Operator dev wiki

If `../../wiki` exists, read its `index.md` first, and record decisions and
run results there (`decisions.md`, a thread page). Nothing private from the
wiki goes into this repository.
