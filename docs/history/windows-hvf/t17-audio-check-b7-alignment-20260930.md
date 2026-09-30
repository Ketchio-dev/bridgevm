# T17 audio check aligned with B7 — operator decision

Classification: a test-definition change made on an operator decision, with
deterministic evidence (automated tests, evidence rank 3). The current A9
criterion and product state come from `capabilities/windows-hvf.json`. No
criterion is promoted; A9 remains OPEN.

## Why the check changed

[r50](a9-r50-audio-counter-gate-20260930.md) showed that T17's host audio
check could not pass after any guest shutdown. It required
`callback_errors == 0`, while every shutdown produces callback statuses the
host types as expected (`EnqueueDuringReset` while the queue stops): three in
each r49 and r50 stop, and 30 across B7's ten-run receipt. B7's proven
statement requires zero unexpected callback errors with expected shutdown
statuses separately typed and counted. A5 requires `frames_rendered>0` and
`drops==0` and does not mention callback errors.

Because this changes a pass threshold after a failure was observed, it was
put to the operator. On 2026-09-30 the operator chose to align T17's check
with B7's proven semantics rather than keep it or change the host.

## The check now

T17 accepts the final host report's CoreAudio stats only if they parse under
B7's exact ordered field set with every B7 reconciliation holding. It then
requires:
- `frames_rendered > 0`;
- `drops == 0`;
- `queue_stop_errors == 0` and `queue_dispose_errors == 0`;
- `callback_unexpected_errors == 0`;
- `callback_errors == callback_expected_stopping_errors`, so every callback
  status is one the host typed as an expected shutdown status.

The queue stop and dispose conditions, the exact field set and the
reconciliations are stricter than the old check. The legacy three-field line
now fails closed.

The Swift helper (`T17AudioCounters.swift`) and the Python tier verifier
(`t17_audio_counters.py`, which reuses B7's own parser in
`scripts/audio-teardown-result.py`) apply the same rule. r50's actual final
counters pass both. The same line with one unexpected, dropped, queue-stop or
unreconciled count fails both.

## Limit

This changes only T17's journey check. A5's and B7's verifiers and statements
are unchanged. No live run has used the new check yet.
