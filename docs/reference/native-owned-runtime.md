# Native owned runtime control

This reference describes the implemented macOS app ownership and stop protocols.
User commands and exit codes are documented in [Native app CLI](../app-cli.md)
and the [stop guide](../app-cli-stop.md).
The contracts below concern retained processes, cleanup observations and media leases.
They do not establish normal Windows shutdown, saved guest work, guest health,
or a release gate result. Product claims remain in
[windows-hvf.json](../../capabilities/windows-hvf.json).

## Two protocol boundaries

| Boundary | Transport | Schema and operations |
| --- | --- | --- |
| CLI → running app | Private Unix socket, one request per connection | Read-only status v1; separate control v2 `stop` / `stopStatus` |
| App → retained runner | Dedicated stdin/stdout pipes | `schemaVersion: 1`; `hello`, `stop`, and sequenced runner events |

The existing status request `bridgevm.app-runtime-request.v1` and response
`bridgevm.app-runtime.v1` retain their `app-observation` scope and APIs.
Control uses `bridgevm.app-runtime-control-request.v2` and
`bridgevm.app-runtime-control.v2`, with scope `app-owned-runtime-control`, on the
same endpoint. The command result `bridgevm.app-stop.v1` is a separate CLI envelope.
The runner mode is `--owned-runtime-stdio`; protocol stdout carries framed events.

Both transports prefix each JSON payload with its four-byte big-endian length.
Decoders compare the payload with its canonical re-encoding: reordered keys,
whitespace, duplicate or unknown fields, and noncanonical values are rejected.
Runner payloads use ASCII, lowercase UUIDs and explicit nulls for optional fields;
outer app messages use canonical uppercase UUIDs and their Swift encoder's shape.
Neither protocol is newline-delimited JSON. Inner events permit exactly one
payload matching `kind`, and unknown kinds fail validation.

## App admission and durable operations

The endpoint is selected from the account and the physical library directory
identity. Its private directories and socket are checked by retained file identity,
type, owner and mode; peers must have the same effective UID. The owner retains a
permanent lock file and verifies its lock/socket/library ownership around requests.
This is a local account ownership boundary, not a remote authentication protocol.

A stop request binds the library identity, canonical VM ID, app instance UUID,
run token, retained runner PID and accepted registration digest. That digest
identifies the source VM registration captured for the session; it is distinct
from the launch-manifest digest used by the runner. Later registration edits do
not retarget an accepted run. Missing registrations can still have retained runs.

The MainActor router rechecks cancellation, the connection deadline and current
owner identity immediately before ledger admission. It uses an existing model
and exact retained session, without creating a model, attaching, scanning for a
replacement process, or launching the app. Unsupported legacy launches and attached
observations are refused. Multiple owned candidates are ambiguous. An attached
observation cannot be hidden by selecting an older, completed owned run.

The ledger reserves the operation before callbacks or effects. Repeating an ID
with its exact target returns the retained operation; reusing it for another
target is refused. A fresh ID for the same retained target returns the canonical
existing operation ID, which subsequent `stopStatus` requests must use.
Tickets remain queryable after client disconnect, model removal or later session
replacement. They live only in that app instance; no restart recovery is implied.
An unknown `stopStatus` never creates an operation. Capacity refusal never evicts
an existing ticket, and a reentrant request during reservation has no extra effect.

## Runner identity and cleanup evidence

`hello` has sequence zero and binds a fresh run token to the exact launch-manifest
SHA-256. The runner checks that digest before media admission or child creation.
For encrypted TPM state, the key is carried only in this initial pipe message;
it is not an argument, environment value, outer response or diagnostic field.
The committed golden key fixture is synthetic test data, not operational state.

`ready` repeats the digest and declares the cancellation timing. Runner events
carry the run token, runner PID and contiguous sequence numbers starting at one.
The app rejects a wrong identity, sequence gap, duplicate terminal event or event
after `complete`. Helper child identities include their reset generation;
each generation must be reaped before another begins. The TPM has one generation.
The ledgers match `childStarted` and `childReaped` identities and totals, rather
than inferring descendant exit from the runner's exit.

One `stop` operation uses command sequence one. Admission records its ID before
publishing `stopAck` and exposing cancellation to the runtime. Repeated identical
commands are idempotent within the input limit; a different admitted ID invalidates
the channel. A natural terminal event can race stop admission: a matching run's
`complete` without an operation ID remains valid when no stop was acknowledged.
The app drains buffered completion when STOP races a closed input pipe.

`complete` reports child totals, last reaped identities, outcome, failure code,
media lease disposition and runtime-directory disposition. `releasedAfterReap`
is accepted only for an admitted run; `notAdmitted` requires no child starts.
The final summaries must match the full observed lifecycle. Cleanup failure,
including directory removal failure, cannot become a successful stop response.

## Completion, cancellation and retained ownership

A successful app stop requires a validated cleanup `complete` **and** exit of
the exact retained runner process. The output stream must also reach clean EOF,
and any queued guest shutdown append must have returned. Until then,
`awaitingRunnerExit` may legitimately contain both cleanup and runner-exit records.
EOF prevents a buffered duplicate or malformed terminal frame from escaping checks.
Runner exit alone, presentation state `stopped`, and `stopAck` prove no cleanup.

If ready, the app allows a guest shutdown grace interval. Once the guest service
is observed, it attempts one `shutdown.exe /p /f` append through the descriptor
whose regular-file identity was captured at launch. The worker rechecks both the
path and descriptor; replacement, symlink, hard link or failed delivery leads to
owned runner cancellation. It never recreates or reopens a replacement control file.
A pre-READY stop skips guest grace. Guest command delivery is not guest shutdown proof.

Runner cancellation can follow STOP, owner input EOF, a signal or protocol failure.
Each retained child receives TERM, then KILL if needed, with actual reap observation.
Wait uncertainty suppresses further PID signalling; it does not abandon the child
or media leases. Expired reap observation reports `cleanupUnconfirmed` and continues
retaining ownership. Missing cleanup evidence blocks replacement and destructive
library operations even after the wrapper exits. Terminal state publication also
guards against a synchronous observer starting a replacement during that transition.

Stop phases are `guestGrace`, `cancelling`, `awaitingRunnerExit`, `completed` and
`unconfirmed`. The first failure remains sticky. The controller records when all
final conditions were first observed, so a delayed callback cannot turn a missed
deadline into success. Later valid cleanup can release the ownership fence while
the original operation remains failed. Protocol corruption keeps proof invalid.
The runner itself returns nonzero for cancellation; the app CLI can return zero
for that cancellation only when its complete cleanup proof satisfies every condition.

## Fixed bounds

| Limit | Current value and meaning |
| --- | --- |
| App request / response payload | 8,192 / 65,536 bytes; v1 status at most 32 sessions |
| App exchanges | 2 seconds each; four admitted connection slots, retained until handler and I/O finish |
| App ledger | 256 operations per app instance; retained records are not evicted |
| Guest grace / final observation | Up to 180 + 12 monotonic seconds; cancellation can shorten, never extend, the deadline |
| CLI wait | One 210-second monotonic deadline including initial status; individual exchanges keep their 2-second bound |
| Runner payload / event buffers | 8,192 bytes; Rust queue plus in-flight frame at most 32; Swift event buffer 32 |
| Runner control input | At most 16 STOP frames, including duplicates |
| Pipe I/O | 2-second HELLO, partial-frame, write and final-drain bounds; overflow or invalid framing fails closed |
| Child cancellation | 2-second TERM grace plus 2-second KILL/reap observation; uncertainty retains ownership afterward |

CLI transport retries preserve the target and admitted operation. A lost submit
reply is queried before retrying the same unconfirmed admission ID. The wait never
falls back to an attached runtime or starts an owner. A late reply is not success.

## Source and deterministic checks

- Outer schemas and validation: [control DTOs](../../apps/macos/Sources/BridgeVMControl/NativeRuntime/NativeRuntimeControlProtocol.swift), [codec](../../apps/macos/Sources/BridgeVMControl/NativeRuntime/NativeRuntimeControlCodec.swift), [phase validation](../../apps/macos/Sources/BridgeVMControl/NativeRuntime/NativeRuntimeControlValidation.swift).
- Admission and waiting: [app router](../../apps/macos/Sources/BridgeVMControl/NativeRuntime/NativeRuntimeAppControlRouter.swift), [operation ledger](../../apps/macos/Sources/BridgeVMControl/NativeRuntime/NativeRuntimeStopLedger.swift), [CLI waiter](../../apps/macos/Sources/BridgeVMControl/NativeCLI/NativeCLIStopWaiter.swift).
- Inner protocol: [golden payload index and hashes](../../tests/fixtures/owned-runtime-v1/index.json), [Rust codec](../../runners/hvf-runner/src/owned_protocol_codec.rs), [Swift codec](../../apps/macos/Sources/BridgeVMControl/HvfEngine/HvfOwnedRuntimeCodec.swift), [lifecycle ledger](../../apps/macos/Sources/BridgeVMControl/HvfEngine/HvfOwnedRuntimeLedger.swift).
- Ownership implementation: [controller](../../apps/macos/Sources/BridgeVMControl/HvfEngine/HvfOwnedRunController.swift), [guest append worker](../../apps/macos/Sources/BridgeVMControl/HvfEngine/HvfOwnedGuestShutdown.swift), [retained child wait](../../crates/bridgevm-hvf-runtime/src/owned_child_wait.rs).
- Boundary checks: [socket contracts](../../tests/integration/native-runtime-transport-contract.py), [admission](../../apps/macos/Tests/BridgeVMControlTests/NativeRuntimeControlAdmissionTests.swift), [response validation](../../apps/macos/Tests/BridgeVMControlTests/NativeRuntimeControlCodecTests.swift), [CLI waiting](../../apps/macos/Tests/BridgeVMControlTests/NativeCLIStopTests.swift).
- Lifecycle checks: [controller faults and deadlines](../../apps/macos/Tests/BridgeVMControlTests/HvfOwnedRuntimeControllerTests.swift), [retention](../../apps/macos/Tests/BridgeVMControlTests/HvfOwnedRuntimeRetentionTests.swift), [actual runner pipe contracts](../../runners/hvf-runner/tests/owned_protocol.rs), [paired Rust/native CLI route](../../apps/macos/Tests/BridgeVMControlTests/HvfOwnedRunnerCLITests.swift).

These tests use synthetic media and harmless child fixtures, including an actual
runner/CLI route without App.main or a live VM. Source and passing deterministic
tests do not replace commit-specific hosted CI or physical-hardware gate receipts.
