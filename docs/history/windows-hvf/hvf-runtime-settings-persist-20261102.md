# Save Windows HVF runtime network and shared-folder choices (2026-11-02)

The runtime view's "Virtio net" toggle and shared-folder selection were view
state only. Dashboard Start, native CLI start and app relaunch rebuild the
launch from vm.json, which defaulted the guest NIC on and never shared, so a
guest taken offline came back online and a chosen share was dropped.

A library VM now saves both in vm.json the way clipboard sync is saved: only
while fully stopped, through work admission, the unattached disk-process
check, registration currency and the publish outcome, rebinding the stopped
session in place. `networkEnabled` is written only when its effective value
changes. `sharedFolderHostPath`/`sharedFolderGuestPath` are written together
from an explicit act: sharing off, a chosen folder, Return in a share field,
or runtime-view Start with a typed pair. Only an existing absolute directory
is saved; a saved folder missing at launch fails readiness (`share-invalid`)
and nothing else is shared. Sharing is never on by default and a clone does
not inherit its source's share. The path, not the directory identity, is
saved (the app is not sandboxed and stores other user paths the same way);
the runtime still pins the root only within one run. Older builds drop the
new keys when they save vm.json.

Evidence rank: static reasoning plus new XCTest cases using temporary
directories and no VM. At this commit they have not been compiled or run.
No live run; no criterion claim.
