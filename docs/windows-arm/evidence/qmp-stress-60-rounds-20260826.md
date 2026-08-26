# QMP test-socket flake: matched baseline plus 60 clean stress rounds

The QMP acceptance gate has two halves: reproduce the original failure with the
old socket ordering in the same session, and then complete 60 consecutive
`cargo test --workspace --locked` rounds under the same load with zero
failures. Both halves are now met by one retained receipt.

## What ran

Studio live-queue job `20260825-182319-70701-218`, tier `t10-qmp-stress`, at
exact code head `8c7f45df90fe4c02b5392ff0fea01486dff64f7b`, on Mac16,9 /
macOS 26.5.2 with 24 load processes running throughout.

## Result

```
result=pass exit_code=0
baseline_iterations=20 baseline_matches=20
sample_count=60 required_run_count=60 passes=60 failures=0
load_processes=24 outcome=completed pass=true
```

Receipt SHA-256 `9ab6ca4133133bd57263a7983b13766c96059b01252fbba3f73ad27381b0f6a1`;
redacted public receipt SHA-256
`2946a2d9dee5306f389cbd092fee27d6e33bccb3b2e46ddb2bf6ce0ce8d638e3`.

The retained `baseline.txt` records `baseline_iterations=20` and
`baseline_einval_then_econnrefused=20`: the old one-shot socket order still
produces the exact `EINVAL` → `ECONNREFUSED` sequence 20 out of 20 times in
this session, so the reproduction is matched rather than assumed.

All 60 round logs are retained. Independent audit of the retained evidence,
not just the summary: decompressing each of the 60 logs and scanning every
`test result:` line finds zero failed tests in any round, and all 60 contain a
full ~880-test workspace pass.

## Why the earlier attempts failed and what changed

No production timeout, retry interval or QMP client behaviour was changed for
this result. Every earlier campaign failed in a *test fixture*, and each was
repaired in the fixture:

- `9/60` — `reconcile_children_bootstraps_guest_tools_session`: the fake child
  `sleep 5` exited inside the existing ten-second observable wait and the test
  indexed a missing key. Parked on piped stdin instead.
- `19/60` then `3/60` — the daemon test store's `/tmp/bvmd-<pid>-<counter>`
  namespace collided under sustained load because each process reset its
  counter and PIDs are reused. Replaced with an atomic `create_dir`
  reservation; 26,822 stale roots were deliberately left in place so the fix
  is exercised rather than hidden by a clean environment.
- `3/60` — command-result readiness, guest benchmark observation and
  nonterminal QMP metadata all supervised `sleep 5` children. Parked and
  polled against the real reconcile path.
- `2/60` — `reconcile_children_cleans_up_terminal_qmp_event`: the one-shot fake
  server coupled its greeting to the production supervisor's 25 ms retry tick
  and panicked on the retryable client's EOF. Connection/negotiation was
  separated from the behaviour under test: the fixture now pre-negotiates a
  real `QmpClient` with a bounded test-only setup timeout, verifies the actual
  `qmp_capabilities` command, parks the child and prebuffers terminal
  envelopes before polling the real supervisor cleanup.

Each of those fixtures is a genuine observability defect in the test, not a
tolerance that was loosened: production's 25 ms supervisor timeout, the
`QmpClient` and the ten-second observable waits are unchanged, and the
verifier still requires all 60 retained decompressed logs to match.
