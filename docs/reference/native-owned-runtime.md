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
| CLI → running app | Private Unix socket, one request per connection | Read-only status v1; separate control v3 `stop` / `stopStatus`; start v2 `start` / `startStatus` |
| App → retained runner | Dedicated stdin/stdout pipes | `schemaVersion: 1`; `hello`, `stop`, and sequenced runner events |

The existing status request `bridgevm.app-runtime-request.v1` and response
`bridgevm.app-runtime.v1` retain their `app-observation` scope and APIs.
Control uses `bridgevm.app-runtime-control-request.v3` and
`bridgevm.app-runtime-control.v3`, with scope `app-owned-runtime-control`, on the
same endpoint. The command result `bridgevm.app-stop.v1` is a separate CLI envelope.
The runner mode is `--owned-runtime-stdio`; protocol stdout carries framed events.

Both transports prefix each JSON payload with its four-byte big-endian length.
Both ends are BridgeVM binaries, so decoders ignore unknown fields, key order
and spacing; the size limits still apply. The app routes a request by its
`schema` field. Runner payloads use ASCII and lowercase UUIDs; outer app messages
use canonical uppercase UUIDs. Neither protocol is newline-delimited JSON. Inner
events permit exactly one payload matching `kind`, and unknown kinds fail
validation. Run token, runner PID, sequence and the lifecycle are checked
exactly; a failure code, cause or reason the app does not know is reported as a
cleanup failure, not a protocol error.

## App admission and durable operations

The endpoint is selected from the account and the physical library directory
identity. Its private directories and socket are checked by retained file identity,
type, owner and mode; peers must have the same effective UID. The owner checks its
permanent lock file (regular, 0600, owned by the account, one link) once when it
takes the `flock`, and holds that lock for the whole app session. Each request is
decoded and checked once by the server (frame size, schema, library identity, a
canonical VM ID, the app instance ID); handlers trust it afterwards. This is a
local account ownership boundary, not a remote authentication protocol.

A start request names only the library, VM ID, app instance and operation ID. The
app starts the VM's current saved configuration through the GUI start path, so a
CLI start and a Start click share admission, readiness checks, key handling and
error text. Start failures carry that path's sentence in the observation's
`failure`. Start and stop refusals use only `ownerChanged`, `operationUnknown`
(the CLI then retries its admission) and `refused`, and always carry one plain
sentence in `reason`, which the CLI prints.

A stop request binds the library identity, canonical VM ID, app instance UUID,
run token, retained runner PID and accepted registration digest. That digest
identifies the source VM registration captured for the session; it is distinct
from the launch-manifest digest used by the runner. Later registration edits do
not retarget an accepted run. Missing registrations can still have retained runs.

The MainActor router uses an existing model
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
An unknown `stopStatus` never creates an operation, and a reentrant request during
reservation has no extra effect. The start and stop ledgers keep the newest 256
finished operations for status queries and drop older finished ones; operations
still in progress are always kept, and history never refuses a new request.

## Runner identity and cleanup evidence

`hello` has sequence zero and binds a fresh run token to the exact launch-manifest
SHA-256. The runner checks that digest before media admission or child creation.
For encrypted TPM state, the key is carried only in this initial pipe message;
it is not an argument, environment value, outer response or diagnostic field.
The committed golden key fixture is synthetic test data, not operational state.

`ready` repeats the digest; its cancellation timing is informational. Runner events
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
is observed, it attempts one `shutdown.exe /s /t 0` append through the descriptor
whose regular-file identity was captured at launch. If the service is not observed
when the stop is admitted, the app first presses the guest's ACPI power button once
(`POWERBUTTON` in `input.ctl`); the grace, the Force Stop offer and the later agent
request are unchanged. A confirmed Force Stop may add
one `shutdown.exe /p /f` and cancels the runner 30 seconds later. The CLI's stop
requests runner cancellation when the grace ends; the app's Stop and quit keep
waiting and offer Force Stop instead. The worker rechecks both the
path and descriptor; replacement, symlink, hard link or failed delivery leads to
owned runner cancellation. It never recreates or reopens a replacement control file.
A pre-READY stop skips guest grace. Guest command delivery is not guest shutdown proof.

Runner cancellation can follow STOP, owner input EOF, a signal or protocol failure.
Each retained child receives TERM, then KILL if needed, with actual reap observation.
The HVF helper treats TERM as a stop: it ends the run through its final report,
which saves the UEFI vars and NVMe media (write-back ends with `F_FULLFSYNC`).
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
| App ledger | Newest 256 finished operations per app instance; in-progress ones are always kept |
| Guest grace / final observation | Up to 180 + 12 monotonic seconds; cancellation can shorten, never extend, the deadline |
| CLI wait | One 210-second monotonic deadline including initial status; individual exchanges keep their 2-second bound |
| Runner payload / event buffers | 8,192 bytes; Rust queue plus in-flight frame at most 32; Swift event buffer 32 |
| Runner control input | At most 16 STOP frames, including duplicates |
| Pipe I/O | 2-second HELLO, partial-frame, write and final-drain bounds; overflow or invalid framing fails closed |
| Child cancellation | TERM grace (8 seconds for the HVF helper, which saves its media; 2 for swtpm) plus 2-second KILL/reap observation; uncertainty retains ownership afterward |

CLI transport retries preserve the target and admitted operation. A lost submit
reply is queried before retrying the same unconfirmed admission ID. The wait never
falls back to an attached runtime or starts an owner. A late reply is not success.

## Source and deterministic checks

- Outer schemas and validation: [control DTOs](../../apps/macos/Sources/BridgeVMControl/NativeRuntime/NativeRuntimeControlProtocol.swift), [codec](../../apps/macos/Sources/BridgeVMControl/NativeRuntime/NativeRuntimeControlCodec.swift), [phase validation](../../apps/macos/Sources/BridgeVMControl/NativeRuntime/NativeRuntimeControlValidation.swift).
- Admission and waiting: [start ledger](../../apps/macos/Sources/BridgeVMControl/NativeRuntime/NativeRuntimeStartLedger.swift), [app router](../../apps/macos/Sources/BridgeVMControl/NativeRuntime/NativeRuntimeAppControlRouter.swift), [operation ledger](../../apps/macos/Sources/BridgeVMControl/NativeRuntime/NativeRuntimeStopLedger.swift), [CLI waiter](../../apps/macos/Sources/BridgeVMControl/NativeCLI/NativeCLIStopWaiter.swift).
- Inner protocol: [golden payload index and hashes](../../tests/fixtures/owned-runtime-v1/index.json), [Rust codec](../../runners/hvf-runner/src/owned_protocol_codec.rs), [Swift codec](../../apps/macos/Sources/BridgeVMControl/HvfEngine/HvfOwnedRuntimeCodec.swift), [lifecycle ledger](../../apps/macos/Sources/BridgeVMControl/HvfEngine/HvfOwnedRuntimeLedger.swift).
- Ownership implementation: [controller](../../apps/macos/Sources/BridgeVMControl/HvfEngine/HvfOwnedRunController.swift), [guest append worker](../../apps/macos/Sources/BridgeVMControl/HvfEngine/HvfOwnedGuestShutdown.swift), [retained child wait](../../crates/bridgevm-hvf-runtime/src/owned_child_wait.rs).
- Boundary checks: [socket contracts](../../tests/integration/native-runtime-transport-contract.py), [start admission](../../apps/macos/Tests/BridgeVMControlTests/LibraryModelOwnedRuntimeStartTests.swift), [response validation](../../apps/macos/Tests/BridgeVMControlTests/NativeRuntimeControlCodecTests.swift), [CLI waiting](../../apps/macos/Tests/BridgeVMControlTests/NativeCLIStopTests.swift).
- Lifecycle checks: [controller faults and deadlines](../../apps/macos/Tests/BridgeVMControlTests/HvfOwnedRuntimeControllerTests.swift), [retention](../../apps/macos/Tests/BridgeVMControlTests/HvfOwnedRuntimeRetentionTests.swift), [actual runner pipe contracts](../../runners/hvf-runner/tests/owned_protocol.rs), [paired Rust/native CLI route](../../apps/macos/Tests/BridgeVMControlTests/HvfOwnedRunnerCLITests.swift).

These tests use synthetic media and harmless child fixtures, including an actual
runner/CLI route without App.main or a live VM. Source and passing deterministic
tests do not replace commit-specific hosted CI or physical-hardware gate receipts.
