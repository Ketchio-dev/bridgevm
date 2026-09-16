# Start an app-owned native VM

```sh
bridgevm app start 개발-vm
bridgevm app start 개발-vm --json --library "/absolute/path/to/native-library"
bridgevm app status 개발-vm --json
bridgevm app stop 개발-vm --json
```

Use the exact ID from `bridgevm app list`. The app must already own the selected
library and retain its library model. Start supports saved, installed own-HVF
entries. It does not open an app or window, install a guest, attach to an existing
process, repair a registration, or use a legacy wrapper. See [CLI setup](app-cli.md).

The command binds one operation to the app instance, canonical library identity,
VM ID and normalized saved configuration digest. The retained model must match
the saved configuration. Changes to either, an unsupported backend, a pending
installation, or competing runtime/storage work refuse admission. Edited runtime
options in an existing session must also match the saved launch configuration.
TPM recovery/reset and snapshot work reserve the same session before beginning.

Exit 0 requires a validated supervisor READY message and initial helper startup
for the accepted run, with no earlier terminal or protocol failure. This is a
startup milestone, not proof of guest boot, display readiness or guest health.
The milestone remains historical evidence if that run later stops; query `status`
for current app observations. No release criterion is closed by this command.

The CLI observes for up to 30 seconds from invocation. The app separately fixes
its startup deadline at 30 seconds from the first accepted admission. The initial
status exchange consumes part of the CLI window. Each transport exchange has its
existing two-second deadline; a lost response is queried and, only if admission
is unknown, retried using the identical operation and binding. No retry renews
a deadline or selects a new owner. Starting a new CLI invocation creates a new
operation and never aliases an earlier start.

Disconnecting or timing out the CLI does not cancel admitted app-owned work.
A pending synchronous key or preparation call remains retained until it returns.
Late results cannot launch a replacement or turn missed-deadline failure into
success. Possible owned processes remain reserved until cleanup is established.
The app retains at most 256 start records for its lifetime without evicting old
ones. At capacity, new admissions fail; existing records remain queryable.
App restart does not restore these operation tickets.

Start reads an existing TPM key without creating, updating or deleting a key,
including when no TPM state exists yet. An inaccessible or absent selected key
returns failure. A resolved Keychain backend is reused; otherwise a read-only
entitlement inspection chooses one candidate. A missing key does not trigger a
cross-backend fallback. Historical transient backend fallback was not persisted,
so absence describes the selected backend rather than every possible store.

The data-protection lookup uses a fresh context with interaction disabled.
Legacy lookup saves, disables and restores the process interaction policy under
a shared BridgeVM key-operation gate, restoring before consuming the result.
Busy access fails promptly. A restoration failure discards the key and disables
that access service. The legacy policy is process-global; unrelated framework
calls do not participate in the BridgeVM gate. Injected tests establish control
flow, not platform-wide proof that no prompt can appear.

JSON uses `schema: "bridgevm.app-start.v1"` and `scope: "app-owned-runtime-start"`.
Fields include `vmID`, `libraryPath`, `appInstanceID`, `operationID`,
`expectedSavedConfigurationDigest`, `started`, `observation` and
`unavailableReason`; absent optional fields may be omitted. Observations include
phase, fixed deadlines, worker/process ownership, failure and startup proof.
The proof's launch-manifest digest is distinct from the saved-config digest.
Wire start/startStatus messages use their own strict version 1 schemas on the
same private endpoint; existing status and stop schemas remain unchanged.

Exit codes: 0 for confirmed startup, 1 for refused/unavailable/unconfirmed startup,
and 2 for invalid usage. Admission alone never yields successful exit status.
