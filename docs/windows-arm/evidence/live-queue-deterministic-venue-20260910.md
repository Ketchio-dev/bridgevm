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
