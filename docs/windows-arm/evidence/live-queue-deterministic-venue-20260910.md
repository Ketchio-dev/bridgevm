# Deterministic checks do not belong in the physical-Mac queue

Date: 2026-09-10. Code: `b8335961`. This is an execution-venue contract,
not a Windows guest gate or a product-state promotion.

## Observed policy gap

The queue CLI accepted `t0-check`, and the sealed tier dispatcher invoked
`scripts/check-project.sh` for that tier. That path contradicted AGENTS.md:
ordinary deterministic checks belong on GitHub-hosted Actions, with bounded
local developer checks permitted separately. A physical Mac is not needed
merely because a deterministic check is slow.

## Implemented boundary

- Submission rejects `t0-check` with exit status 2 before creating queue or
  job-ledger state.
- Dispatch rejects it with the same explanation before creating output or
  invoking the project check. This also covers a previously queued t0 job.
- Existing manifest-refusal and queued-cancellation fixtures use `t1-vtimer`;
  those tests submit or cancel only and do not execute a hardware tier.
- Historical t0 records are not removed or rewritten.

## Deterministic evidence

`tests/integration/live-deterministic-venue-smoke.sh` passed 6 checks locally.
The dispatcher test uses a copied dispatcher and a harmless sentinel project
check. A regression cannot recursively run the real suite or start a VM.
Both refusal statuses, explanatory diagnostics, and absence of side effects
are checked. The contract is called by the existing live-queue policy suite.

The preceding checkpoint `9c627a6034e225c95af85061be71051b42a9b2f6` has green
hosted [CI 34434435071](https://github.com/Ketchio-dev/bridgevm/actions/runs/34434435071),
[Security 34434435127](https://github.com/Ketchio-dev/bridgevm/actions/runs/34434435127),
[B6 collector contracts 34434435250](https://github.com/Ketchio-dev/bridgevm/actions/runs/34434435250),
and [B6 frame-time contracts 34434435158](https://github.com/Ketchio-dev/bridgevm/actions/runs/34434435158).
These runs do not validate the later venue change. Its full project check
and exact-checkpoint hosted runs must be recorded separately.

A9, A11, and B6 remain OPEN. This change proves no installation, reboot,
glyph matrix, renderer performance, or release criterion.

## Scope correction during B6 queue integration

The statement above that dispatch refusal also covers a previously queued t0
job was too broad. The worker checks out the job's sealed SHA. A job sealed
at a revision before the refusal change would still use that older dispatcher.
The six focused checks prove rejection by the revised CLI and dispatcher,
not retroactive enforcement over older sealed revisions. A worker-level
venue boundary remains to be implemented and tested; no old t0 job was run
on the physical Mac to investigate this gap.

## Worker-level implementation after the correction

Code `705e1a1c` adds the refusal to the parent worker before it resolves or
fetches a job's sealed revision. This is independent of the dispatcher in
that revision. Two deterministic worker tests passed locally in 1.329 seconds:
`t0-check` is refused before any Git call for two unresolved SHA fixtures,
while an allowed hardware tier still reaches the fake revision resolver.
The original six CLI/dispatcher checks also pass. Fake repositories, queue
commands, Git, and disk-space responses keep these tests away from real VM
execution or network fetches.

The full project check and exact-checkpoint hosted results for this later
change are still pending. The installed LaunchAgent worker has not yet been
updated with this guard. Its deployment is deferred while the separately
sealed B6 observation job is running. Source implementation must not be read
as a claim that the deployed worker already enforces the new boundary.

The full local `scripts/check-project.sh` subsequently passed for code commit
`705e1a1cf4bedc5f9830937013964f5374612ea4` (log retained privately as
`codex-worker-venue-20260910/project-check.log`). Installed-worker deployment and
exact checkpoint hosted checks remain pending at this record. The installed
worker was idle with a clean worktree; that does not itself prove deployment.

Checkpoint `97589c0f7866288733ab7235395530d567cb9bc1` passed exact hosted CI
34439581741, Security 34439581832, collector 34439581847, FrameTime 34439581801,
active collection 34439581819 and sealed cell 34439581755. After the 150-percent
B6 job terminated and process inspection found no worker/VM, the clean installed
worker clone was fast-forwarded to that SHA with its LaunchAgent stopped and
then bootstrapped again. Installed worker script SHA-256 is
`92cb8ed0ace5915fe628f573656a7a4305ee90b0203ee8c6dfb16e06a8042367`. The worker-level t0 guard is now
deployed; no forbidden deterministic job was submitted to physical hardware to
exercise it. The existing synthetic worker contracts remain the refusal evidence.
