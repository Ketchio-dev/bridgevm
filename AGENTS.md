# AGENTS.md — how work is done in this repository

These are binding rules for anyone, human or agent, changing this repository.
They exist because this project's failure mode is not bad code; it is
**believing something works when the evidence does not support it**.

Agents work autonomously under the standing authority in section 0. Carry each
change through implementation, relevant checks, failure analysis and repair,
merge and a recorded result without stopping for approval. An elapsed timer is
a checkpoint for judgment, not a reason by itself to stop. Autonomy changes who
decides, never what counts as evidence: the evidence, security and data
boundaries below still apply.

## 0. Standing autonomy

On 2026-10-25 the operator granted standing authority for continuous,
autonomous development. Do not stop to ask for opinions or approval. Choose the
next work yourself: release-blocking criteria and disclosed known defects
first, then the open threads in the operator dev wiki (section 12). When one
item is blocked, record why and move to the next.

Without asking, an agent may:

- merge its own pull request with a merge commit once the exact head passes
  `scripts/check-project.sh`, the GitHub-hosted full project check and every
  required pull-request check, then confirm post-merge `main` CI and repair or
  revert a red `main` before other work;
- submit physical-Mac live-queue jobs (pilots, campaigns, diagnostics) for a
  pushed SHA, record each run, and clean up the lane state it created:
  clones, scratch, retained sources and lane vTPM keys;
- take product decisions inside the current product scope, such as limits,
  refusals, defaults and user-interface behaviour within registry wording,
  recording each with its reason in the wiki's `decisions.md` and, when
  user-visible, in the repository history;
- record a criterion as PROVEN when a retained receipt meets its unchanged
  statement at the stated sample count on the stated head, and retract one
  when the evidence no longer supports it;
- delete merged or patch-equivalent branches and worktrees after distilling
  anything the wiki requires;
- continue across sessions: before stopping, leave the state, running job ids
  and the next step in the wiki so the next agent resumes without asking.

Ask the operator only for what an agent cannot or must not do itself: spending
money or creating accounts; credentials, certificates and passwords; physical
presence such as a console unlock, reboot or operating-system installation;
TCC or other host security-policy changes; legal or licensing judgments;
publishing a release or changing public release assets; changing the
operator's own data, canonical images or other assets that no agent lane
created; changing `product_state`; and changing a criterion's statement,
threshold or scope. Ask once and concretely, keep the dependent criterion open,
and continue other work while waiting.

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

Agents never change a criterion's statement, threshold or scope. The operator
may, as an explicit dated scope decision recorded in the registry history. It
applies only to measurements taken after the decision and never turns an
earlier failure into a pass.

## 3. Claim discipline

- Retracting a claim is normal work. State plainly that a previous conclusion
  was wrong, and leave the failed experiment in the record.
- Never rewrite a failed experiment into a success.
- User-facing capability wording comes from `capabilities/windows-hvf.json`.
  Do not invent alternative phrasing in README, STATUS, the CLI or the app.
- The product state lives in `capabilities/windows-hvf.json` (`product_state`)
  and may never be promoted while a release-blocking criterion is unproven.
  Graphics are an *experimental Vulkan path* and an *experimental
  D3D11-compatible subset*. A non-blocking criterion with a live user-visible
  defect stays OPEN and its `known_defect` wording is surfaced by generation —
  never hidden by reclassifying the criterion as satisfied-by-recording.
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

- Make a concise plan when a change crosses subsystems or affects a release,
  security, or guest-contract boundary, record it in the wiki thread, and
  execute it. A plan is not a request for approval; only the items listed in
  section 0 go to the operator.
- Commit per proven conclusion, not per experiment. A commit message states
  what is now known, not what was attempted.
- After pushing, confirm hosted CI is green for that SHA. Red CI blocks
  promoting any live result to release evidence.
- Remove all temporary instrumentation before ending a session.
- `GOAL.md`, `PLAN.md` and `HANDOFF.md` are operator-owned and are not staged.

## 9. Long tests use the correct execution venue

- Local deterministic developer checks, including `scripts/check-project.sh`,
  may run past **300 seconds** when they are making useful progress. At 300
  seconds, inspect progress and machine load, then choose a finite next
  checkpoint. Continue, move the check to GitHub-hosted Actions, or stop with
  the reason recorded. A local result does not replace the required
  GitHub-hosted CI result for a pushed SHA.
- Run required deterministic CI for a pushed SHA on GitHub-hosted Actions and
  record the run id; local checks provide development feedback. **Never submit
  an ordinary deterministic check to the physical-Mac live queue merely
  because it is slow.**
- The physical-Mac live queue (historically called the Studio queue) is
  reserved for live work that genuinely requires a physical Apple-silicon
  Mac, bare-metal Hypervisor.framework, WindowServer/CGL, private Windows
  media or a real GPU. Record its job id.
- `sleep 6000`, long `while ps; sleep` loops and multi-hour synchronous tool
  calls are prohibited.
- Poll hosted CI in bounded intervals. After 180 seconds, reassess whether to
  keep following that run in this turn or record its id and return later; the
  elapsed time alone does not decide when the task ends.
- A submitted live-hardware job seals its commit, binary, image and vars
  hashes, so later edits cannot change what it measured.
- Fast tiers filter candidates. Only the release tier produces shipping
  evidence.

## 10. CI boundary

- Everything deterministic runs on **GitHub-hosted** Actions.
- Only work needing a physical Apple-silicon Mac, bare-metal
  Hypervisor.framework, WindowServer/CGL, private Windows media or a real GPU
  runs on the physical-Mac live queue. The particular Mac that drains this
  queue is test infrastructure, not a product runtime or build dependency.
- **No self-hosted runner is attached to this public repository.** A workflow
  reachable from a fork must never execute on a personal machine.
- Moving an ordinary check off hosted CI to hide a failure is forbidden.

## 11. Before you finish

Run the deterministic project check:

```sh
scripts/check-project.sh
```

Nothing is "done" while that check fails.

## 12. Operator dev wiki

The operator may keep a private BridgeVM dev wiki at `../../wiki`, relative to
this checkout. It is for BridgeVM work only. When it exists:

- pull it and read its `index.md` before starting;
- search it before guessing a past run, decision or machine fact;
- after a run or decision, update it under the rules in its own `AGENTS.md`,
  recording every decision taken under section 0 in its `decisions.md`.

The wiki ranks below this repository. Product truth stays in
`capabilities/windows-hvf.json`, and nothing private is copied from the wiki
into this repository. When `../../env.sh` exists, source it before building;
it puts that machine's toolchain on PATH.
