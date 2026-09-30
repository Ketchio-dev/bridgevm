# Private install staging and host-bound stop record — deterministic

Classification: deterministic development evidence (automated tests, evidence
rank 3). Current criterion wording and product state come from
`capabilities/windows-hvf.json`. No criterion changes; A9 and B9 remain OPEN.

## Install media in shared /tmp

The packaged app's Windows ISO install placed its 64 GiB target, its UEFI
variables and its evidence directory at fixed names derived from the VM slug
in shared, sticky `/tmp`. Another local user could pre-create or link those
names, two installs with one slug collided, and the evidence directory was
never removed after a successful install. On 2026-09-29 a T17 lane left it
world-readable (directory 0755, a 7.4 MB run log and install frames 0644), and
the variables file was 0644 as well.

On the unmodified source, 8 of 12 new staging and journal tests failed, as did
both cases of a new source contract; the four guards passed. The one predicted
failure that did not reproduce is recorded: an atomic variables write replaced
a planted link instead of writing through it.

Each install attempt now stages its media in a 0700 directory inside the VM
bundle. The app creates the target exclusively at the requested size and
writes the variables without following links. Staging is removed through the
directory descriptor, never through a path that could be a link, and after
commit only the bundle's install log remains, including on the
library-reconcile path. A bundle or metadata directory that other users can
write is refused with a message that names the permission. Journals sealed by
older builds over the two legacy `/tmp` names stay resumable only for regular
files owned by this user. A test runs the real scripted-install runner with
the app's arguments on paths containing spaces. After the change 16 staging
tests, the runner test and the contract passed.

## SYSTEM_OFF matched by text alone

The HVF runtime prints guest command output and, after its final report, the
guest serial tail verbatim into `run.log`, so an exact
`stop: PSCI 0x84000008 (system off)` line can come from the guest. On the
unmodified source, the T17 guest-evidence verifier accepted 8 of 8 forged
first-run logs, the B9 raw-order check 3 of 3 and the shutdown helper 9 of 9;
5 of 6 new contract cases and 4 of 5 new Swift cases failed.

T17, import and B9 consumers now accept the stop record only as the one the
final host report frames, whose serial-tail byte count must match the bytes
that follow. One definition per language implements this, and Swift and Python
give identical results on all 742 retained report-bearing logs. Every forged
fixture is refused. The four retained logs in the current count format bind at
exactly the offsets the old matcher chose; 711 older logs use a legacy count
format that the production grammar does not accept, which fails closed. The
runtime output format did not change.

Still open: shell and regular-expression readers elsewhere accept the stop
text anywhere, audio counters are read without host framing, and guest PDB
paths reach the report unescaped. A capture after a stable-log window, not
after helper exit, could still bind a complete forged report.

## Limit

Neither change has run on physical hardware. They add no journey pass, and
all bounds and pass conditions are unchanged.
