# AGENTS.md — how work is done in this repository

These are binding rules for anyone, human or agent, changing this repository.
They exist because this project's failure mode is not bad code; it is
**believing something works when the evidence does not support it**.

## 1. Evidence hierarchy

Claims are ranked. A lower-ranked result never overrides a higher-ranked one.

1. **Live gate receipts** at the stated sample count on real hardware.
2. **Live single runs** — useful signal, never a criterion pass.
3. **Automated tests** — necessary, but they do not prove guest behaviour.
4. **Static reasoning** — generates hypotheses only.

A single passing run does not close a criterion that asks for 9 of 10. Read the
failing state — framebuffer, symbols, instruction bytes, timer and mask state,
owning-thread GIC snapshots — before proposing a cause. A plausible narrative
that explains the symptom is not evidence.

## 2. Never relax a criterion to pass it

Thresholds may be raised, never lowered, and never redefined mid-investigation
to match what was achieved. If a gate cannot be met, the criterion stays open
and the product wording stays honest. Substituting a smoke benchmark for a real
workload is the same violation.

## 3. Claim discipline

- Retracting a claim is normal work. State plainly that a previous conclusion
  was wrong, and leave the failed experiment in the record.
- Never rewrite a failed experiment into a success.
- User-facing capability wording comes from `capabilities/windows-hvf.json`.
  Do not invent alternative phrasing in README, STATUS, the CLI or the app.
- The product is **Engineering Preview** until every release-blocking criterion
  is proven. Graphics are an *experimental Vulkan path* and an *experimental
  D3D11-compatible subset*.
- The guest platform is a *QEMU `virt`-compatible contract with documented
  deviations*, not a bit-for-bit copy. New guest-visible differences require an
  entry in `docs/machine-contract/qemu-virt-deviations.json`.

## 4. Security fails closed

- Never advertise a security feature that is not implemented. A guest-visible
  RNG must come from the host CSPRNG; a provider failure returns an error
  status rather than a predictable value.
- Never derive security-relevant values from exit counts, time, PIDs or
  constants.
- Release builds must not honour repository or PATH overrides for helper
  binaries.
- Media whose key provenance cannot be established is classified as
  development-only and is not promoted by an audit that only sees timestamps.

## 5. Image and data safety

- Canonical images are immutable. Live runs use `cp -c` clones.
- Cross-volume `cp -c` does not preserve APFS clone relationships; stage
  external sources into an internal sparse cache first.
- Each parallel lane gets its own cloned disk **and** its own vars file. A
  shared writable vars file invalidates the run.
- Guest disks, UEFI vars, vTPM state, recovery keys and third-party title
  content never enter git or CI artifacts. Receipts carry hashes and counts.

## 6. Guest asset conventions

- Files under `scripts/win-assets/**` keep CRLF line endings.
- Normalise guest logs with `tr '\r' '\n'` before parsing.
- Agent protocol: append to `agent.ctl` only after the service starts, deliver
  via the share plus `powershell -File`, launch workloads through
  `Invoke-CimMethod Win32_Process Create`, wait by filename, and keep individual
  shared files under about 8 MB.

## 7. Structural budgets

`scripts/refactor-budgets.tsv` ceilings are a ratchet. **Never raise an existing
ceiling.** When a file would exceed its budget, extract a module instead.
New files are registered at their actual size.

## 8. Planning and commits

- A change spanning three or more files needs an approved `PLAN.md` first.
- Commit per proven conclusion, not per experiment. A commit message states
  what is now known, not what was attempted.
- After pushing, confirm hosted CI is green for that SHA. Red CI blocks
  promoting any live result to release evidence.
- Remove all temporary instrumentation before ending a session.
- `GOAL.md`, `PLAN.md` and `HANDOFF.md` are operator-owned and are not staged.

## 9. Long tests run asynchronously

- **Do not wait in the foreground for anything expected to exceed 120 seconds.**
  Submit it to the Studio local queue and record the job id.
- `sleep 6000`, long `while ps; sleep` loops and multi-hour synchronous tool
  calls are prohibited.
- Poll hosted CI for at most 180 seconds, then record the run id and read it on
  a later turn.
- A submitted job seals its commit, binary, image and vars hashes, so later
  edits cannot change what it measured.
- Fast tiers filter candidates. Only the release tier produces shipping
  evidence.

## 10. CI boundary

- Everything deterministic runs on **GitHub-hosted** Actions.
- Only work needing bare-metal Hypervisor.framework, WindowServer/CGL, private
  Windows media or a real GPU runs on the local Studio queue.
- **No self-hosted runner is attached to this public repository.** A workflow
  reachable from a fork must never execute on a personal machine.
- Moving an ordinary check off hosted CI to hide a failure is forbidden.

## 11. Before you finish

Run the deterministic project check:

```sh
scripts/check-project.sh
```

Nothing is "done" while that check fails.
