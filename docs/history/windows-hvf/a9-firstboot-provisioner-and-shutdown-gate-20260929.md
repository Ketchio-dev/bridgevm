# A9 first-logon provisioner and shutdown gate — deterministic

Classification: deterministic development evidence (hosted and local automated
tests, evidence rank 3). The current A9 criterion, defect wording and product
state come from `capabilities/windows-hvf.json`. A9 remains OPEN; no physical
run has exercised these changes.

## Why this was examined

In the [r35 observation](a9-r35-first-boot-oobe-20260929.md) and in r33, the
third boot generation reached a running agent-console driver with port 1 ready
and host-open, yet no guest process ever opened the port. Reading the path
that should start that process found two defects that block the ISO-install
journey regardless of timing.

## The first-logon provisioner refused every valid receipt

Both the default product unattend and the T17 unattend run
`scripts/win-assets/bvagent-firstboot.ps1` at first logon. It must verify the
staged guest-payload receipt, register the agent task and start it. Each
receipt record is a string array, and the schema and architecture lookups
assigned a single match unwrapped, so its count was the field count, 2, and the
script refused. Only its asset-hash function had ever been tested.

A new hosted contract runs the unchanged script as FirstLogonCommands does,
under Windows PowerShell 5.1 and PowerShell 7, against a receipt in the exact
staging format, with a stub task module. On the unmodified script,
[run 36634503525](https://github.com/Ketchio-dev/bridgevm/actions/runs/36634503525)
failed with `BVAGENT PROVISION BLOCKED: guest-payload receipt schema is
invalid` for the valid receipt. Wrapping both lookups in `@()` made the valid
receipt stage the task while duplicate, missing and wrong records and a
mutated agent are still refused.
[Run 36634604251](https://github.com/Ketchio-dev/bridgevm/actions/runs/36634604251)
printed PASS for both interpreters but failed the step on the last child's
intentional exit code; after the contract exited with its own verdict,
[run 36634735501](https://github.com/Ketchio-dev/bridgevm/actions/runs/36634735501)
passed.

So no product-flow install could have registered the agent task. That this is
why r32, r33 and r35 saw no READY is inferred, not observed: no guest
provisioning log was retained from those runs.

## The shutdown gate matched a line the runtime never prints

The runtime prints a clean guest power-off as
`stop: PSCI 0x84000008 (system off)`. The T17 proof, guest journey and stop
waits, the installed-disk import stop wait, the guest-evidence verifier and
the B9 pilot all matched `stop: PSCI SYSTEM_OFF`. That text occurs in none of
1,800 retained live run logs; the exact record occurs once in each of 714. A
build of the unmodified matchers rejected the retained T20 r7 log that reached
READY and shut down cleanly.

Swift and Python now each use one definition of the exact record, compared by
whole-line equality. A contract rebuilds the record from the Rust constants and
format strings and checks every definition, and the retired literal is kept as
a negative fixture in each language. The fixed build accepts the T20 r7 log and
still rejects r35. The first-boot diagnostic's `system_reset` count now counts
reset stop records rather than the per-generation reboot-limit banner. This
matches the same event exactly; no pass condition is loosened.

Two related defects remain open. The host prints the guest serial tail after
its own stop record, so a guest could emit an identical line after it; binding
the record to host output is a design decision. Several app input and window
inventory paths detect a restart by `PSCI_SYSTEM_RESET`, which the runtime
never prints either.

## Private setup-state harvest for first-boot failures

When a lane fails without first READY, the tier now clones the lane disk with
`fclonefileat`, validates its GPT, records whether each partition is NTFS or
BitLocker, and, when readable, attaches and mounts the Windows volume
read-only. It copies a fixed allowlist into the private packet with per-item
and total size caps and hashes: the provisioning log, marker and payload receipt, the agent log
and task definition, the setup state, Panther and UnattendGC setup logs, the
device-installation logs and four event logs, 16 items in all. It then unmounts,
detaches and removes the clone. A harvest failure never changes the lane
result. If the release cannot be proven, the tier's cleanup check now fails
closed as `cleanup-failed` and leaves the clone for the operator. A queue
cancel can still kill the harvest before release; the fence then keeps the
queue stopped.

Synthetic GPT and attach contracts passed, and one local read-only run against
an APFS clone of an installed Windows 11 disk, not a T17 lane, retained 11 of
16 items and released cleanly.

## Related network fix

The same change set makes NAT TCP sockets nonblocking before `connect()`; a
fixed-arity `fcntl` binding had left their flags dependent on stack contents.
The ISO-install journey includes a network stage.

## Limit

None of this has run on physical hardware. The 600-second first-READY bound and
every pass condition are unchanged. A9 needs a passing sealed journey for both
ISO install and installed-disk import on a clean machine; A9 and A11 remain
OPEN and product state stays ENGINEERING_PREVIEW.
