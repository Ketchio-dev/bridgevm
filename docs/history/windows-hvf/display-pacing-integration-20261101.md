# App display and ordered-input pacing diagnostics (2026-11-01)

Continuation from main `a66009af`, integrating the unfinished display lane’s
three commits (`8c17765f`, `5648c021`, `ecffb427`) by cherry-pick. Existing
A9/audio changes are retained. This record distinguishes *static reasoning*
and *automated tests* from live evidence.

## Measurements and boundaries

- The host-only BVFB header carries a publish timestamp at bytes 48..56.
  Writers stamp completed pixels inside the existing seqlock; readers take
  the timestamp with the same frame. Zero means unknown for older writers.
  The guest contract is unchanged.
- The app counts display-link outcomes, skipped sequences, frame holds,
  content age at the target vsync, tick work and main-thread busy stretches.
  Assignment to a layer and the display link’s target timestamp do not prove
  actual presentation or whether Core Animation met the target.
- Ordered-input diagnostics count admissions, coalescing, queue depth,
  discards and send-to-inserted-receipt round trips. Round trips include the
  app’s polling delay and do not establish guest input-to-present latency.
- Fixed-size counters are recorded once per ended runtime session, with no
  per-event log and no input text. T17/T19 retain the first run’s records in
  private lane results. The reader summarises these beside exporter timing.
- Missing diagnostics do not affect a journey outcome; their values do not
  satisfy a release criterion. No guest disk, vars, keys or title content
  is added to git or CI artifacts.

## Review and verification

Two independent static reviews found malformed JSON could raise an uncaught
TypeError, and binding changes could discard queued input without counting
it. The first-run reader also checked a pathname before reopening it, leaving
a link-replacement race. Integration repairs address these with regression
coverage. No routing behavior, criterion threshold or product state changes.

Seal `0b8ad5e5` failed hosted production-input-driver run 41332195527:
the explicit source list omitted the observer extension. It is repaired; the
obsolete local full check was stopped with its log retained.
Final local/hosted results go in the wiki. No live campaign ran during LoRA training.
A9, A11 and A19 remain OPEN.
