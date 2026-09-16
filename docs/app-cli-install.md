# Control a saved Windows installation from the CLI

```sh
bridgevm app install 개발-vm
bridgevm app install-status 개발-vm --json
bridgevm app install-cancel 개발-vm
```

Use the exact ID from `bridgevm app list`. A compatible app must already be
running and own the selected native library. These commands do not open a window
or launch another VM lifecycle.

`install` starts or reuses the pending installation request saved when the VM was
created. It never accepts an ISO path, password, recovery key, unattended secret,
or driver path on the command line. Those inputs remain in the app-owned VM
bundle and its existing recovery flow.

The app binds the operation to its current instance, the canonical library, the
exact VM ID, and the normalized saved-configuration digest. It admits only a
saved own-HVF Windows VM whose `installPending` value is true. A changed or
ambiguous registration, running VM, UI-started installation, storage mutation,
or other reserved work refuses admission.

Plan construction runs outside the main actor because older saved requests may
need to hash their ISO. The app reserves the operation first and retains that
reservation if the CLI disconnects. Before creating a session it checks the app
owner, library identity, saved configuration, request metadata, and plan again.
A changed input produces a terminal failure and no installation session.

`install-status` reads the retained phase, cancellation availability, failure and
a bounded tail of recent host installation logs. `install-cancel` requests
cancellation only while the retained operation permits it. Cancellation during
plan preparation prevents session creation. Once finalization or recovery owns
installation media, the existing recovery rules decide what must be preserved;
the CLI cannot force-delete media or journals.

The initial `install` command returns after the running app accepts or finds the
operation. It does not wait for Windows installation to finish. Repeating the
same internal operation ID is idempotent, and concurrent requests for the same
VM observe the one retained operation. A later explicit invocation can create a
new retry only after the previous operation is terminal and the exact pending
configuration still matches.

Text and JSON report app-owned host installation state. They do not prove guest
boot, display readiness, driver behavior, or any release criterion. Automated
protocol and model tests likewise do not replace live hardware evidence.

JSON uses `schema: "bridgevm.app-install-command.v1"` and
`scope: "app-owned-install"`. It includes the command, VM and library identity,
app instance, requested operation ID, disposition, observation and refusal when
available. Observation phases are `preparingPlan`, `validating`,
`preparingSource`, `installing`, `finalizing`, `recovering`, `cancelling`, `done`,
`failed`, and `cancelled`.

Exit codes are 0 when the requested control exchange is accepted or observed, 1
when it is refused or unavailable, and 2 for invalid usage. An `install` response
already carrying a failed or cancelled observation returns 1. A successful exit
from `install` means work was accepted or found; it never means Windows finished
installing.
