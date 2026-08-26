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

## Runs 7–8: the launch wedge mechanism, named from guest evidence

Two more lanes ran after the retraction above. They sharpen the diagnosis from
"intermittent" to a named mechanism.

In every failing lane the retained guest `bvagent.log` *stops writing entirely*
at the resize reply, while the host keeps printing its own 30-second
`BVAGENT SERVICE alive` heartbeat. That heartbeat is a host print, not a guest
reply (`resident_service.rs:21`), so it is not evidence the agent was healthy —
the agent was blocked, not idle.

`bvagent.ps1`'s `Invoke-B64` runs every command as
`cmd.exe /c $cmd 2>&1 | Out-String`. `Out-String` returns only once every
inherited handle closes, and a GUI child keeps the agent's stdout open for its
whole lifetime. That is why all three earlier substitutions failed: each
changed the verb, none changed the handle. The B4 pointer case has always
launched through `> file 2>&1` for exactly this reason.

Run 8 (`4568416`) applies that shape — `start "" notepad.exe >
C:\BridgeVMPtr\notepad.out 2>&1` — and failed differently and earlier, at
`helper exited 1 before agent`. Its evidence is a separate guest problem, not
the launcher: generation 1 recorded `agent_handshake=false`,
`stage4_marker_present=0` with `firstboot_fresh=1`, and its `run.log` ends with
the guest still in `BdsDxe: starting Boot0003 "Windows Boot Manager"` when the
720-second boot watchdog fired. Six of the eight lanes reached
`agent_handshake=true`; this one never booted far enough to try.

So two distinct failures are now separated: a launch that wedges the
single-threaded agent through an inherited pipe (addressed), and guest boot
flakiness after the reset boundary (not addressed, and not a scripting error).
The criterion stays OPEN with no captured scene.

## Runs 9–11: the launch is fixed; the console stall is not the launcher

Run 10 (`874205d`) moved the Notepad launch out of the agent console and into
`bvgpu-apply-host-resolution.ps1`, using the same `Invoke-CimMethod` mechanism
that has always launched the B4 pointer target from that script's own process.
It worked: the lane printed

```
BVNOTEPAD_LAUNCHED return=0 pid=5304
```

the agent stayed responsive, and the run reached `WINLIST` for the first time
in eleven lanes. It then wedged on `SERVICE overdue share-get`, because the
redirect wrote `notepad.out` into `C:\BridgeVMPtr` — the synchronised share —
so the host began fetching a file the guest still held open. Run 11 moves that
output to `C:\BridgeVM`, outside the share, exactly where the B4 target's own
`.out` file lives.

Run 11 then failed earlier still, at `guest resize failed`, with only the
initial `whoami` completing. That is the same console stall appearing one
command sooner, and it is what the ten retained lanes now show as a whole:

| stalled at | lanes |
| --- | --- |
| after `whoami` (resize never returned) | 1 |
| after resize (next command never returned) | 6 |
| after Notepad launch (`WINLIST` never returned) | 1 |
| reached typing | 1 |
| never booted | 1 |

The stall is not attached to any particular command. Each fix moved the run
further and the stall reappeared at whatever command came next, which
falsifies "this verb blocks the agent" as a general explanation and points at
the guest console/agent servicing under load after the reset boundary.

The launch defect itself is real and fixed — `BVNOTEPAD_LAUNCHED return=0` is
direct evidence — and so is the text-size defect from run 1. The criterion
stays OPEN: no scene has been captured, and glyph correctness remains
`unmeasured`.
