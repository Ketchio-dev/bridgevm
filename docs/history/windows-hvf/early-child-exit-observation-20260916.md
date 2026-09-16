# Early child exit observation — 2026-09-16

Historical deterministic evidence; the [registry](../../../capabilities/windows-hvf.json)
remains authoritative. No guest or native UI capability is promoted.

PR163 source 07f4138 failed hosted run 35095980418 in the CLI doctor fixture
and two GUI cancellation cleanup checks. The earlier PR-list snapshot exposed
only 100 checks and did not establish a green full CI result.

A private forced-delay socket experiment reproduced macOS accept inheriting
nonblocking mode: read returned WouldBlock before request arrival. Explicitly
clearing that mode preserves the original two-second read/write deadlines.
The three CLI doctor tests pass; no CLI product behavior was changed.

Unmodified local GUI cancellation tests passed before child creation. A private
copy inserted 200ms of scheduling delay after channel activation to exercise
cancellation after swtpm spawn. Both cases failed cleanup: one child spawned and
reaped with signal15, no helper, no child startup marker or kqueue witness.
The validated runner ledger reported cleanup complete and the OS reported both
exact child PIDs absent. The original hosted log lacks this additional detail;
the experiment reproduces its failure shape, not its unrecorded exact schedule.

The test fixture now requires either its registered live exit witness or the
validated one-spawn/one-reap record plus independent OS ESRCH for that exact PID.
Live or reused PIDs, permission errors, missing statuses and ambiguous counts
refuse cleanup. Signal zero observes existence; it does not terminate a process.
No runtime logic, timeout, assertion about guest shutdown, or release gate changed.

The same private scheduling experiment then passed both cases plus four evidence
regressions in 13.200s. Missing-reap negative cases use a real already-exited PID.
Its log SHA256 is 66432f9a67f5429dc179afd79676611c6374a1ad913961f99008b7a9a0ec19df.
The experiment runner continues after failure to collect both cases; its shell
exit code alone is not a pass. The retained RED log explicitly reports two failures.
No scheduling delay or diagnostic print was added to repository source.

Full-project and exact-source hosted validation are pending for this change.
Private diagnostic logs, input hashes and prior failures remain in packet54 evidence.
