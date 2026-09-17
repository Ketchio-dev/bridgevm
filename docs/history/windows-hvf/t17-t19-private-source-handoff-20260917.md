# T17-to-T19 private source handoff — 2026-09-17

Source checkpoint: `a851a2e4fa950087a5d37c882279367ccc97b002`

## Problem

The installed-disk import gate needs a disk, UEFI variables, vTPM state,
recovery package, and recovery code from one real stopped Windows VM. Existing
external assets did not contain that complete set. Using the active user VM
would have risked user data, while T17 previously deleted its successful lane
before T19 could reuse it.

## Change

After the final T17 lane passes the existing host authentication and no process
owns its temporary root, the tier can now retain one private T19 source. It:

- exports the existing vTPM key through the exact packaged app;
- APFS-clones the installed disk and 64 MiB UEFI variables;
- copies a bounded regular-file-only vTPM tree;
- writes the existing T19 manifest format and re-runs its verifier; and
- makes the retained tree read-only, with its parent private and recovery code
  inaccessible to group or other users.

The live worker chooses `$HOME/BridgeVM/t19-sources/<job-id>`, outside the queue
directory that moves from `running` to `done`. An existing destination is never
overwritten. Guest media and recovery material remain private and are absent
from public receipts, git, and CI artifacts.

## Deterministic evidence

The Windows product E2E contract suite passed. Its synthetic authenticated T17
lane produced a T19 manifest that the existing import verifier accepted. A
second lane targeting the same destination was refused without overwriting the
first source. T17 still emitted its own honest receipt when handoff failed.

This proves only the deterministic handoff and failure boundaries. It does not
prove an ISO installation, imported Windows boot, BitLocker unlock, guest
shutdown, clean-machine operation, or A9 completion. A9 remains OPEN and both
release journeys remain 3D-off.
