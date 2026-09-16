# Native app installation CLI — 2026-09-16

Classification: historical deterministic evidence. Product state remains in the
[registry](../../../capabilities/windows-hvf.json). A9 and A11 stay OPEN and
ENGINEERING_PREVIEW is unchanged.

## Implemented boundary

The native app owner now accepts `install`, `install-status` and
`install-cancel` requests over its existing private socket. The paired Rust CLI
exposes the same operations under `bridgevm app`. Each request binds the exact
canonical library, app instance, VM ID, normalized saved-configuration digest
and operation ID before admission.

Installation media paths, passwords and recovery keys are never accepted on
the command line. `install` can only consume the pending request already saved
inside that VM's private metadata. The app retains operation state, reconciles
it against the exact saved configuration and rejects changed, ambiguous,
unsupported or concurrently busy targets. Retry is idempotent while an
operation exists and a new attempt is admitted only after a terminal result.

Plan preparation runs outside the main actor. The owner and exact library,
model and saved-config digest are checked again before a session is created.
The admitted installation excludes only itself from the shared work guard;
runtime, storage, UI installation and other CLI work remain conflicts.
Cancellation stays visible until the retained installation session reaches a
terminal state. Logs and failure text are bounded by the wire codec.

## Retained validation

| Check | Result |
| --- | --- |
| Focused Swift protocol, store, model, router, admission and CLI suites | 25 PASS |
| Rust app CLI unit suite | 15 PASS |
| Rust real-process install integration | 2 PASS |
| Private socket transport | 22 PASS using the production framing/router source set |
| Full workspace Rust tests | PASS; the real CLI help regression was corrected to the current install wording |
| XCTest shim suites | 425 app, 789 control and 62 Apple VZ PASS; two required live-only skips |
| Structural budgets | PASS without raising a ceiling |
| First integrated project check | FAIL only capability freshness and rustfmt; every executable suite completed, and the failure is retained |
| Corrected format and focused CLI check | PASS at source `51bd5426521ac689aeae28317009e156dcc1e1eb` |

The transport build initially omitted the new install protocol source files and
therefore failed compilation. The source list now includes the protocol, codec,
observation validation and routing files; its 22 cases pass. Two bounded-size
assertions initially used an XCTest helper absent from the repository shim.
They now express the identical predicates through the supported assertion and
all three shim suites pass. These failures are not rewritten as successes.

## Evidence limit

These checks prove deterministic request validation, admission, retained state,
process forwarding and private-socket behavior. They do not run App.main, open
a WindowServer UI, read real installation secrets, install Windows, boot a
guest, observe guest shutdown or prove post-install retry on physical hardware.
No live queue job was submitted because this packet is deterministic. Hosted
exact-head validation remains required before integration, and the final local
project check is recorded on the documentation checkpoint. No criterion,
threshold, product wording or machine-contract deviation is promoted here.
