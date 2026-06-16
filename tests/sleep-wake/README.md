# Sleep/Wake Tests

This lane owns sleep/wake recovery coverage.

The current executable baseline is intentionally metadata-only:

```sh
tests/sleep-wake/metadata-baseline-smoke.sh
```

The smoke creates a disposable VM metadata store, reads CLI status, asks for
suspend and resume lifecycle plans, and writes a bounded baseline artifact under
that temporary store. It also shadows backend runners and `pmset` so the test
fails if it accidentally starts QEMU, starts Apple VZ, or requests host sleep.

Current contract:

- No host sleep is triggered.
- No VM is started.
- No guest networking, display, or resume behavior is claimed.
- The baseline records stopped VM metadata before and after sleep/wake planning.
- Fast Mode suspend/resume planning remains non-executable until the live backend
  is implemented.

Future live coverage should add an explicit opt-in lane for host sleep/wake with
a running VM and should validate guest reachability, display recovery, clock
reconciliation, and resumed lifecycle state after wake.
