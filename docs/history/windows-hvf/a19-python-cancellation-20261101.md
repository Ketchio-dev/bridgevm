# A19 auxiliary cancellation on Python 3.15 (2026-11-01)

## Failure and reproduction (automated)

While validating the display pacing integration at `0b8ad5e5`, hosted T20
run [41332195384](https://github.com/Ketchio-dev/bridgevm/actions/runs/41332195384)
failed `test_term_unwinds_and_cleans_the_active_owned_command_tree`: the
fixture driver remained alive past its unchanged five-second wait.
This is a failed deterministic check, not a guest disk-safety sample.

The auxiliary SIGTERM handler raised `InterruptedError`, an `OSError`
subclass. CPython 3.15’s macOS `_wait_kqueue` catches `OSError` around its
kqueue wait and falls back to another wait. The exception therefore failed
to unwind the active owned command and reach cleanup promptly. See the
[upstream implementation](https://github.com/python/cpython/blob/v3.15.2/Lib/subprocess.py).

An isolated experiment used that exact upstream method with a real local
kqueue and a synthetic sleep child. With the old handler it returned false
and swallowed the cancellation; with the repaired handler the exception
propagated. A modeled wait-boundary regression also failed before repair.
The original hosted failure and both experiment results remain recorded.

## Repair and limits

- A dedicated `AuxiliaryInterrupted` exception derives from `RuntimeError`,
  outside the operating-system error hierarchy. The existing CLI error
  boundary reports it; command cleanup still runs while unwinding.
- Synthetic process-tree fixtures publish readiness after installing TERM
  handlers. Timeout tests wait for startup before the unchanged 0.5-second
  command limit; a failed startup is not confused with cleanup failure.
- The regression verifies propagation through the wait boundary and that
  owned cleanup runs. The existing real TERM test passes within five seconds.
- No cleanup deadline, uncertainty fence, criterion statement, sample count
  or product state changes. Similar handlers elsewhere need separate audit.
These are automated results only. No Windows lifecycle or interrupted
operation gate sample is added. Exact-source hosted and full local checks
remain required; A9, A11 and A19 stay OPEN.
