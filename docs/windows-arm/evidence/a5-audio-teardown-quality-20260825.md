# A5 audio teardown quality: ten-run live sample, callback_errors=0

The 2026-08-17 A5 re-proof met the criterion (`frames_rendered>0 AND drops==0`)
but carried an open quality note: a single boot printed `callback_errors=3` at
stream teardown. That note is now closed by a fixed ten-run live sample.

## What ran

Studio live-queue job `20260825-165255-89865-16467`, tier `t9-audio-teardown`,
at exact code head `2965b38bcda7758831aef32d494f7b89f03ae202`. The tier's
sample count is fixed at ten; it is not a threshold that a run may lower.

Each run clones the canonical immutable media into its own private copy. The
sources are mode-400 `uchg` regular files and hashed to the values recorded in
the receipt:

```
image_sha256 3ff89519de32863c1e301074beba0347b5431929ec4b6f8bb70accf775338b86
vars_sha256  a17c31f291967e00802a9c50a740566bb903a2e1528a76704234ac09ac59ee5f
```

## Result

`result=pass`, `exit_code=0`. The receipt records:

```
sample_count=10 required_run_count=10 passes=10 failures=0
callback_errors=0 frames_rendered=4224747
outcome=completed pass=true
```

Receipt SHA-256 `1594c6c334753db5f38c726612dc9399aa378eac7397caa70cdbb770dfac82c0`;
redacted public receipt SHA-256
`549f5d7c213502e8f83a704c6f1b7670c515e8739931e7f3f5d35c7da9199723`.

Every one of the ten retained per-run summaries was read directly. All ten
record `a5_audio=pass`, `drops=0`, `callback_errors=0`,
`unexpected_callback_errors=0`, `stop_errors=0`, `dispose_errors=0` and
`guest_sound_device_status=OK`, with per-run `frames_rendered` between 401640
and 440977. Each run also records three separately classified teardown
re-enqueue refusals, which are the expected AudioQueue behaviour after stop and
are counted apart from callback errors rather than folded into them.

## Why this is not a weakened gate

`audio_teardown_summary.passes` still requires `a5_audio=pass`, positive
`frames_rendered`, `drops=0`, `callback_errors=0`, and exact agreement between
`callback_errors` and the sum of `unexpected_callback_errors`, `stop_errors`
and `dispose_errors`. `write-audio-teardown-receipt.py` still reads exactly
`runs/run1`..`runs/run10` and sets `pass` only when all ten qualify; a missing
or malformed summary raises rather than being skipped. No threshold, count or
classification rule was changed for this sample.

## Prior canceled attempts

Earlier t9 jobs `20260825-115604-93681-31826`, `20260825-125649-30638-15001`
and `20260825-143803-21902-18054` were canceled mid-campaign. Each correctly
retained `sample_count=0`, `outcome=failed`, and none contributes any sample to
this result.
