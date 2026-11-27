# Start an app-owned native VM

```sh
bridgevm app start dev-vm
bridgevm app start dev-vm --json --library "/absolute/path/to/native-library"
bridgevm app status dev-vm --json
bridgevm app stop dev-vm --json
```

Use the exact ID from `bridgevm app list`. The app must already own the selected
library and retain its library model. Start supports saved, installed own-HVF
entries. It does not open an app or window, install a guest, attach to an existing
process, or repair a registration. See [CLI setup](app-cli.md).

The CLI sends only the VM ID. The app starts that VM's current saved
configuration through the same start path as the library's Start button: if
`vm.json` changed since the app last read it, the app reads the library again
first, so the CLI always starts what is saved. The start refuses while the VM is
running or starting, while its settings sheet is open (an edit may be about to be
saved), while an installation is pending, and while another runtime or library
operation (snapshot, TPM reset, delete, clone, move, install) holds the VM.

Exit 0 requires a supervisor READY message and initial helper startup for the
accepted run. This is a startup milestone, not proof of guest boot, display
readiness or guest health. The milestone remains historical evidence if that run
later stops; query `status` for current app observations.

The CLI observes for up to 30 seconds from invocation. Each transport exchange has
its two-second deadline; a lost response is queried and, only if admission is
unknown, retried using the identical operation ID. A new CLI invocation creates a
new operation and never aliases an earlier start. Disconnecting or timing out the
CLI does not cancel the start: the VM keeps booting in the app, exactly as if it
had been started there. The app keeps the newest 256 finished start records for
status queries; App restart does not restore them.

A refused or failed start prints the app's own one-sentence reason (the same text
the app shows, including the underlying error text when a launch step fails), for
example a missing Windows disk or an unavailable Keychain key. The TPM key is
handled exactly as for a start from the app.

JSON uses `schema: "bridgevm.app-start.v2"` and `scope: "app-owned-runtime-start"`.
Fields include `vmID`, `libraryPath`, `appInstanceID`, `operationID`, `started`,
`observation` and `unavailableReason`; absent optional fields may be omitted.
Observations include `phase` (`preparing`, `awaitingStartup`, `started`,
`failed`), the owned run `target`, the startup `proof` (launch-manifest digest and
initial helper PID) and the `failure` sentence. Wire start/startStatus messages use
the `bridgevm.app-runtime-start-request.v2` / `bridgevm.app-runtime-start.v2`
schemas on the same private endpoint.

Exit codes: 0 for confirmed startup, 1 for refused/unavailable/unconfirmed startup,
and 2 for invalid usage. Admission alone never yields successful exit status.
