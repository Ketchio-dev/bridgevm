# Glyph scene pilot: first executions, two defects, and one retraction

Until 2026-08-25 no `t11-glyph-scene-pilot` job had ever executed — every
submission was cancelled while queued. Six lanes have now run. None captured a
scene, so the glyph criterion stays OPEN and `glyph_correctness` remains
`unmeasured`. What the six produced is a real defect, a real fix, and a
falsified hypothesis that is retracted here rather than left in the record.

## Run 1 — the text-size defect (`8c7f45d`)

Job `20260825-182319-70689-27534` reached 1600x900, launched Notepad, listed
the window, placed it at `50,60,700,500`, focused it and proved its geometry
and foreground state. It then failed at `fixed text not accepted`.

The host log names the cause exactly:

```
live input rejected: kind=key parse_error=too_long
```

One `KEY text-hex:` command carried the whole 201-byte payload, while
`SETUP_INPUT_ENV_MAX_BYTES` is 128. No glyph was ever typed, so no scene could
be captured. The fix splits the same text across commands that each fit the
unchanged guard; the smoke asserts the reassembled hex is byte-identical to the
original string, that every command stays within 128 bytes, and that the
constant still reads 128, so it fails if a single hex byte is dropped or the
guard moves.

## Runs 2–6 — a launch failure, and a hypothesis that was wrong

The next five lanes all failed earlier, at `Notepad launch failed`. The retained
evidence is consistent across them: the guest `bvagent.log` shows the agent
answering the resize command and then serving only `LS` polls, while the host
prints `SERVICE overdue ctl awaiting-reply=true` until the 300-second wait
expires. The launch command was delivered and its reply never completed.

I attributed that to a blocking launcher holding the single-threaded agent's
console, and substituted three alternatives in turn: `Invoke-CimMethod` with a
bare `notepad.exe`, `cmd /c start`, and `Invoke-CimMethod` wrapping a
redirected `cmd /c start`. Every one failed at the same point.

Checking the hypothesis against the exact case script at each commit falsifies
it. At `8c7f45d` the original command

```
BVAGENT CMD powershell -NoProfile -Command "Start-Process notepad.exe; ..." exit=0
```

completed successfully, and the run went on to prove window geometry. The
launcher was therefore never the defect, and three commits chased a failure
that the evidence says that code did not have. `b036592` restores the original
launcher and keeps only the text chunking, which is the fix run 1's log
actually asked for.

Re-running the restored launcher failed at the same point again, which is the
useful result rather than a disappointment: with the launcher held constant and
the same sealed media
(`d7a95823…` / `c61e2136…`), the same reset shape (`generation=0 exit=42`,
`stable_generation=1`) and the same agent handshake timing (`SERVICE alive` at
t=52865 vs t=53005 ms), one attempt in six succeeds. That makes it intermittent
guest state after the reset boundary, not a deterministic scripting error, and
the next instrument belongs in the guest — not in another launcher rewrite.

## Status

The criterion is OPEN and unmeasured. No blank-glyph claim, in either
direction, is supported by these runs: the declared sample still requires
caption/menu/tab regions at three resolutions and three DPI scales with pixel
masks and a frame-time budget, and none of that has been captured yet.
