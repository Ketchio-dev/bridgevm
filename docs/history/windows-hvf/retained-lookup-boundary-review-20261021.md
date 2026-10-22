# Retained lookup boundary and review — 2026-10-21

This supplements the [retained lookup and resource write record](retained-lookup-and-resource-writes-20261021.md).
It records automated tests and a read-only review. Product state, capability wording,
criterion states, thresholds and known defects remain unchanged.

## Review

An independent read-only review of the repaired lookup found no correctness defect. A
cached retained control lacks its record only after the user dismisses it, and a
dismissed control cannot become active again. Comparing the record's control by object
identity is sound because the record holds the control strongly. HVF-engine controls,
runtime and install records, and the admission paths that read retained records do not
change. An earlier review attempt ended at its deadline without a conclusion and is not
counted.

The review noted that the tenth recovery tests never look up the control between
termination and dismissal. Only the existing runtime-session test covered that side,
and only indirectly.

## Boundary test

`RetainedTerminalUndismissedTests` now pins both sides. A terminal retained control stays
the cached control while its record is undismissed. After dismissal, lookup builds one
fresh ordinary control for the current registration, and the old control stays retained.

| Lookup variant | Boundary test | Recovery tests | Runtime-session tests |
| --- | --- | --- | --- |
| Repaired lookup | 1 of 1 passes | 5 of 5 pass | 10 of 10 pass |
| de5d9de7 predicate restored | 2 assertions fail | 5 of 5 pass | 2 assertions fail |
| Main's unconditional cache | 4 assertions fail | 8 assertions fail | 10 of 10 pass |

The two mutated variants were temporary edits for this measurement and were restored.
They are not part of any commit.

## Unchanged limits

While a terminal record is undismissed:
- Delete, clone and move refuse, and the refusal advises stopping the VM and refreshing
  the list. No list reload is available from there; dismissing the record resolves it.
- Start looks enabled, but it refuses.
- The retained control's status stays frozen.

On main, the same state lasted until the next library reload. This candidate does not
change those messages.

The preceding seal `ce7e5512527d61821b8c886a3402eedf09672181`, which lacks this test,
passed all 45 local stages. The new head requires its own complete local and hosted checks.
