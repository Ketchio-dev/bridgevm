# Visible share-root deferral notice (2026-11-02)

The 2026-11-02 share-root identity guard defers host scans and queued
host-to-guest deletions while the selected Mac folder is unavailable or has
been replaced. That deferral was silent in the normal app: the runtime card
and dashboard showed nothing, and the advanced feed held only raw lines.

The resident share service now prints one line when its selected root first
becomes unusable, `BVAGENT SHARE root-state blocked`, and one line,
`BVAGENT SHARE root-state ready`, after the next complete host scan that the
existing guard accepts. A successful identity match or a delivered queued
deletion alone does not clear the state. Repeated checks print nothing.
Data guards, queues, retries, limits and timing are unchanged.

The app keeps one session-owned notice derived from those exact raw lines.
It survives eviction from the 500-event feed, ignores lines inside command
output frames, and clears on a new start, an admitted stop and a guest reset
record, matching the per-generation runtime that starts unblocked. The
runtime card and dashboard then say that the shared folder cannot be
checked and some synchronization is waiting, with restore or stop-and-
reselect guidance. The notice is informational. It is not an ownership,
security or all-transfers-stopped signal; the backend guards stay
independent of it.

Evidence is automated tests only. Three Rust cases (transition helper,
initially missing root, queued deletion through a replaced root) and six
Swift cases pass. A mutation that let a bare identity match report recovery
failed the queued-deletion case. No live guest, Mac pasteboard or operator
folder was used, and no criterion or product state changes.
