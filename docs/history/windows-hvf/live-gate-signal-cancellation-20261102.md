# Live-gate signal cancellation (2026-11-02)

Five live-gate TERM/INT handlers raised `InterruptedError`, an `OSError`
subclass: the T22 queue adapter, T22 pair preparation, the D5 guest-input
runner, the D4 companion mount and the D11 fixture runtime. CPython's
`selectors.PollSelector.select` returns on `InterruptedError`, so a piped
`subprocess` call with a timeout kept waiting after TERM on every version;
CPython 3.15's macOS `_wait_kqueue` does the same for unpiped waits (see
`a19-python-cancellation-20261101.md`).

Thirteen new tests ran first on unmodified main `0b2c7191` with CPython
3.14.8; 11 failed. Six modeled wait boundaries, one entry-point label and
four real TERMs on piped waits each outlived five seconds. Two passed on
main: the adapter's real TERM (unpiped, so only 3.15/macOS distinguishes
it) and a D11 attempt catch guard.

Each module now raises its own `RuntimeError` subclass. D5 and D11 catch
it where they caught the old error, keeping cancel classification,
receipts, cleanup fences and deadlines. T22 pair failure labels now read
`PairPreparationInterrupted`. All 13 new tests and the existing T22, D11,
D5, HVF stop-reader and D4 contracts pass. No 3.15 interpreter was run;
the A19 exFAT test runner keeps its handler. Automated results only: no
live job, criterion, sample count or product state changes.
