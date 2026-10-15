# Startup recovery, network readiness and clipboard recopy — 2026-10-14

Evidence rank: **test**. This batch follows the
[queue and clone checkpoint](autonomous-queue-and-clone-20261014.md).
Product state, capability wording, thresholds and known defects are unchanged.
A9, A11 and A19 remain OPEN. No guest or physical live result is supplied here.

## Keep ordinary startup recovery off the main actor

Actual cold LibraryModel initialization and explicit reload both ran durable
installation recovery on the main thread. Two tiny synthetic secureBootStaged
fixtures each reached nine recovery seals there; a queued main-actor probe had
not run at synchronous return and ran after yielding. Two methods failed four
context assertions while the disk/vars/journal preservation assertions passed.
This establishes execution context, not a measured stall duration or visible freeze.

Ordinary app entry now performs migration, scan and recovery on one detached
worker. The loading view exposes no model actions until owner/token validation
publishes its result. The native model cache and IPC remain unavailable during
loading; refused commands are not replayed. Concurrent waiters share the worker.
Cancelling a UI waiter does not cancel accepted recovery. Closing invalidates
publication and retains the explicit owner lease until worker acknowledgment,
or process exit. This does not promise a graceful Quit drain or hostile namespace
immunity. Explicit default initialization, reload and add remain synchronous.

Eight new startup methods and 22 related native controls passed. The same eight
new methods passed the actual shim with zero skips; seven DEBUG UI-host controls
also passed. The diagnostic host retains its existing synchronous fixture factory.
No application launch, WindowServer, rendered loading view or TSan result is claimed.
The new lifecycle tests use the new API; they are not byte-identical baseline tests.
Temporary diagnostic instrumentation was removed before implementation.

The original baseline executable was overwritten in reusable build scratch before
archival. Its old hash is historical only. Fourteen other original proof files,
including failed logs and exact diagnostic sources, remain authenticated. The
correction is retained; later fixed executables were copied to immutable paths.
A preliminary test-only semaphore deprecation warning was corrected and its log
retained. These focused checks do not replace integrated project or hosted checks.

## Modern virtio-net waits for DRIVER_OK

Queue enable alone let the network device consume RX buffers populated during
legal initialization, before DRIVER_OK. A premature TX notify and reset followed
by queue reinitialization reproduced the same missing readiness boundary.
The unchanged production baseline had two passing controls and three failures.

RX now checks DRIVER_OK before dequeuing a backend packet; TX checks it before
reading descriptors. Existing queue-ready checks, completion, interrupts and reset
behavior remain. RX delivery moved unchanged into a module to preserve the budget.
This follows [Virtio 1.2 sections 2.1 and 3.1](https://docs.oasis-open.org/virtio/virtio/v1.2/virtio-v1.2.html)
and the modern QEMU path; no intentional guest-contract deviation is introduced.
The transitional legacy early-use exception and other virtio devices are outside scope.

The final public BAR-path fixture uses split 32-bit common-register addresses,
16-bit queue notification, reset/status readback, advertised features and queue
vector readback. Earlier fixture iterations remain in the evidence. Final unchanged
production again passed two/failed three methods with the same three test-source
files used by the repair. All five repaired regressions and 250 related methods
passed; formatting and Clippy also passed.

The first repaired run exposed 11 old packet fixtures left at reset status: 239
passed and 11 failed. Those fixtures now explicitly enter ready state without
changing assertions. An overflow control also enters ready state so it still
reaches overflow. Queue-configuration-only fixtures remain unchanged. This is
modeled device evidence, not guest boot or complete Virtio compliance proof.

## Preserve an observed same-text Mac recopy

After an external changeCount moved, duplicate-text suppression could leave a
previously queued guest write eligible to overwrite the new Mac ownership.
Both host-origin and previous guest-self-write sequences failed through the actual
service, worker and channels. Their executable was archived before rebuilding.

A private checked epoch now advances when the worker observes a suppressed,
nonempty duplicate copy. Guest writes snapshot that epoch when queued; an older
queued write is refused. If no fresh write in the same FIFO batch supersedes it,
the worker sends a fresh host report with a new observation revision. Ordinary
self-write/no-echo behavior and genuinely later guest writes remain supported.
The guest protocol is unchanged. Epoch exhaustion stops the worker without wrapping.

Both reproduced cases now pass. The focused selection passed 177 methods,
including no-conflict recopy, stale/fresh FIFO, no intermediate stale modeled write,
report-revision coherence, fresh-worker isolation and both overflow boundaries.
Non-test compilation, Clippy, formatting and budgets passed; independent review
found no blocker. A fresh batch write means attempted native admission, not provider
success. A reply queued only after the worker observes the recopy is outside the
older-queued-write finding. Empty/nontext, native provider failure, native atomic
check/write, post-drain disconnect and guest application consumption remain unproven.

## Integration and evidence boundary

Parent c2bd3f2a passed all 45 local project stages at 20:22:14 UTC with unchanged
source. Its hosted full run 38628657108 and exact CI remained pending when this
successor was prepared. Those parent results are not this successor's full checks.
The successor still requires its own local project check and exact hosted execution.
All structural ceilings are retained or reduced; new modules use actual sizes.
A local integration script initially committed unresolved budget markers after its
assertion failed. Root corrected and checked the budget before any push; the failed
local state and correction remain recorded. No failed experiment becomes a pass.

| Retained artifact | SHA-256 |
| --- | --- |
| Startup focused receipt | `ecad1d9464257f623a10269e1a33ce1a29bb3e38df21bdda040d68923c95178e` |
| Startup independent review | `d67ed997741a383a7e306f3679af65a1fd5f2d2099648c5947744481565016f4` |
| Network handoff | `92db224e3abc9a03408a5bbe887291e38aec445f1aa59953171eda7c7652de11` |
| Network independent review | `e7d5fc0c2aa7a454fbaf3c42ca760329a11d97053d6529b8a5aae7622ea823d1` |
| Clipboard recopy receipt | `f7cd0e20c81353ec4431c07f8a1dc7fca26db9d1ee7c5bc3088d3a19454e2656` |
| Clipboard independent review | `b4534fef11951962e18564a81389d272c334e71d6ef4b388b6096f3eb53ff7e3` |
| Parent local project log | `2e5e0b0e85606e90ef8f971f9343134b1fe7410caff80655509d1924788df356` |
