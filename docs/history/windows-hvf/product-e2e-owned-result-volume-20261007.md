# Product E2E work on the owned result volume — 2026-10-07

## Reproduced storage boundary mismatch

The T17 and T19 runners allocated every lane below `/tmp`, despite queue,
source and test inputs moving to an external APFS volume. T19 then refused
its own canonical inputs because the source and work devices differed.
T17's deliberate guest-payload hardlink fixture failed before reaching its
isolation assertion when the fixture and lane were on different volumes.

The fixed prefix was enforced across Python request generation, host receipt
authentication, diagnostic packets, cleanup and Swift app/helper admission.
Changing only `mktemp` would either retain these failures or weaken a
security boundary. The actual app also admitted only the T17 prefix although
the import helper uses the same app library option.

## Bound allocation

Each tier now creates work directly below its newly created, canonical,
private result directory, using the exact job identifier and six-character
random suffix. Each request binds the private parent, captured parent
`device:inode`, captured work `device:inode`, and exact lane ordinal. Python
and Swift admission verify owned mode-700 directories, canonical no-symlink
paths, same-device relationships and exact parent/job/lane shape. The app
requires the bounded request beside its empty library rather than admitting
an arbitrary matching temporary prefix. Request exact-key validation remains.

Cleanup uses the parent and work identities retained by the tier before
helper execution, not a replacement root supplied by a helper result. Its
complete descriptor-relative prewalk, device/owner checks, single-link rule,
identity rechecks and mount/process/harvest fences remain in force. T19 keeps
its same-volume input requirement and APFS clone operations. Diagnostic
packets retain original request bytes and verify their hash against the host
stamp, so allocation metadata remains authenticated after work removal and
the normal running-to-done job move. Process checks escape literal path
metacharacters and treat observation errors as residue, not absence.

Deterministic check scripts use explicit TMPDIR templates where BSD mktemp's
no-template behavior otherwise selects internal per-user scratch. Native
runtime sockets remain separate from bulky development and guest work.

## Evidence and limits

Focused synthetic contracts cover T17 installation and T19 import requests,
execution, failure taxonomy and cleanup, plus exact allocation and stale
identity/symlink rejection. Actual Swift request/app admission is exercised
separately because synthetic helpers do not prove Swift behavior.

Exact `78616d3c` local full check FAILED its T22 provenance step: historical
sealed T17/T19 requests lack the new fields. That persistent-data fixture is
valid and unchanged. A narrowly offline validator now admits either exact
historical keys or complete typed current allocation metadata, retaining
request/stamp seals without reading deleted work. Nine original plus two new
contracts pass; successor full/hosted checks remain pending. These are not
Windows receipts. A9/A11/A19 and product state remain unchanged.
