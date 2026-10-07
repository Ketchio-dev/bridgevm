# Live worker launch with external-volume source — 2026-10-07

## Defect and repair

The queue installer regenerated an env-first LaunchAgent with stdout and
stderr below the queue root. On the operator's Mac, moving queue and worker
source to an external SSD exposed two distinct launch refusals: external
launchd log paths failed before startup, and an env-first launch could not
read the external worker even after the operator granted bash disk access.
The machine configuration was repaired separately; reinstalling from source
would have restored both failing settings.

The installer now keeps only launchd stdout/stderr under
`$HOME/Library/Logs/BridgeVM`, with mode 700 on that directory as well as the
queue root. Queue, source, worktrees, media and build paths are unchanged.
The template starts with `/bin/bash -p -c 'exec "$@"' bridgevm-live-launch`
followed by the unchanged `/usr/bin/env -i` argument list. The shell forwards
arguments without reinterpreting paths and the worker still receives only
its explicitly allowed environment. The first shell itself precedes `env -i`;
`-p` disables inherited startup hooks such as `BASH_ENV` before that boundary.
A regression with an injected hook exited 96 without `-p`, not the worker
exit 37; with `-p` the hook is ignored and worker behavior is preserved.

The installer explains that external-volume disk access requires the
operator's permission for bash, separately from helper Accessibility and
Screen Recording. It neither grants permissions nor releases a worker fence.
Scheduling and process-group settings are unchanged. This source change does
not reinstall the agent, alter installed credentials or switch its worker.

## Deterministic evidence

`tests/integration/live-installer-launch-contract.py` uses an owned home,
a spaced checkout path, a simulated external queue and a stubbed launchctl.
It parses the real rendered plist and checks:

- exact bash prefix, sanitized suffix, paths and scheduling settings;
- internal private log directory versus externally backed queue storage;
- argument preservation, inherited override removal and worker exit status;
- the real worker's exit 126 at its existing fence before recovery/claiming,
  leaving fixture queue contents unchanged and releasing its worker lock.

The new suite passes 3/3; existing installer dry-run/refusal tests pass 11/11.
The live-gate policy smoke passes 103 checks. Both project checking and hosted
security policy invoke the new suite. These fixtures do not prove TCC behavior
on other hosts, guest behavior or a release capability. A11 remains OPEN.
