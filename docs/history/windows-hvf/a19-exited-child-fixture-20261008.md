# Deterministic exited-child observation fixture — 2026-10-08

Evidence: synthetic real-process tests and static review, not a live A19 gate.
A verify-loop failure reported TimeoutError where the interrupted-restore
contract expected a helper-exited RuntimeError. The fixture launched a Python
helper that exits1, then assumed its interpreter would start and exit inside
the one-second stage-observation deadline. That branch precondition was not
established before observing.

Root reproduced the same traceback by making the real child delay its exit
for two seconds. This proves the scheduling assumption, not the exact cause
of the original machine delay. The production observer correctly refused an
unauthenticated observation; its timeout, stop checks and cleanup are unchanged.

The test now starts a real child and waits/reaps it before handing its actual
Popen object to the observer. Both immediate and two-second delayed exits
require the exact helper-exited error and no observation/context/FD receipt.
A distinct real live child without staging still requires the exact timeout
and production SIGKILL/reaping. Fixture fallback cleanup runs only after those
assertions and cannot manufacture them. No fake poll/returncode is supplied.

Removing the production exited-child check makes the new slow-exit test fail
with TimeoutError; negative control removed, all seven focused tests pass.
Initial external probe had a sibling-import path error, then was corrected;
its failure remains distinct from the process reproduction. A broader A19
check was interrupted by a90-second command timeout rather than failed
assertions; successor validation remains required. The first delayed baseline
used a short shell sleep, then was repeated with one Python process.

No gate threshold, exception acceptance, release criterion, product state,
installed worker/fence, permissions or user media changed. This arranges a
known-exited-child branch; it does not prove live interruption success.
