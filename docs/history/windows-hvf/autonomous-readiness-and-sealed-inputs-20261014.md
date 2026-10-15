# Public references, device readiness and diagnostic inputs — 2026-10-14

Evidence rank: **test**, with earlier failed live observations retained as context.
This follows the [archive and UI checkpoint](autonomous-archive-and-ui-20261014.md).
Capability wording, state and thresholds are unchanged; A9, A11 and A19 remain OPEN.
Source scope is frozen at `0eba7d9d0aece3e13e92761ab7cdf05b822377bf`; combined full-project
and exact hosted validation for the successor remain pending at this checkpoint.

## Public history must reference published conclusions

Exact `5a846f8aeff5d5bb928d563eccbab430a2812e04` failed hosted documentation references
in run `38639077520`, job `116109688818`: six local author commit objects were unavailable.
The development checkout held those objects, so its local check missed the publication defect.
Integrated `a1f36c3d387412f801c04ff4bc6b5056b2bbda47` substitutes reachable integration
commits and records the failure; the checker, registry and prior 208-line ceiling are unchanged.
The first branch-only clone failed seven references, including a historical commit on another
public branch. After fetching public branches and tags, it reproduced exactly six failures;
the repaired document passed the unchanged checker across 406 current documents. Both failed
iterations remain recorded. This reference repair did not address the separate shim failure.

## Dedicated host tests remain outside ordinary shim discovery

The same candidate failed ordinary Control shim compilation locally and in hosted macOS 15
and 26 jobs. Lexical discovery emitted two references to AppUIInstallCancelFixtureTests,
whose DEBUG && BRIDGEVM_APP_UI_HOST guard excludes it from the ordinary DEBUG-only module.
The local full check ended at 21:37:09 UTC with 44/45 stages passing and all 5,069 source pins
unchanged. The original 1,400-entry runner's missing-class errors remain failures.

Integrated `0eba7d9d0aece3e13e92761ab7cdf05b822377bf` moves both guarded test/support files
byte-identically into the existing diagnostic target. Production Swift, ordinary flags and
release refusal are unchanged. The other 1,398 ordered ordinary entries remain byte-identical;
the fixed runner passed 1,395 with zero failures and three existing skips. All nine dedicated
native methods passed once, and all 23 discovery controls passed. The two moved methods run
with their required host flag. This is a CI routing correction, not a product behavior fix.
An initial budget check failed while the index still listed the old filenames. Staging the
exact moves fixed it; ceilings remain 63/81 and discovery falls from 33 to 28. No GUI is proved.

## Modern console and block alias queues wait for DRIVER_OK

Integrated `7bf7d9e369769f6a96b245b82ce10c1779179f57` guards the modern-only console's
poll and notify paths before queue access. Ordinary polling could consume a legally populated
pre-ready control queue without a premature guest doorbell. Premature notification and reset
followed by reinitialization exposed the same boundary: baseline one passed, three failed.

The modern block guard covers MMIO-register notifications, including modeled PCI BAR aliases.
Its premature notification is a defensive invalid-driver case, not conforming modern PCI
common-configuration startup or live firmware reachability. Baseline two passed, one failed.
Legacy MMIO and separate legacy PCI I/O retain their early queue-use exception. All seven
focused cases passed with the same new fixture bytes used for their respective baselines.
Counters and post-guard processing remain; no emergency-output capability is claimed.

The first broader repaired run passed 239 and failed 18 because old processing fixtures were
still at reset status. They now enter ready state explicitly; all assertions remain, including
malformed-queue bounds, backlog/resumption, descriptor and read-only write rejection. The final
selection passed 257 methods, including the seven focused cases; formatting, Clippy and budgets
passed. This is modeled queue/memory evidence, not actual MSI-X delivery, full port lifecycle,
general Virtio compliance or guest boot proof. Existing ceilings did not rise.

## Generic running-state actions match existing admission

Integrated `b77243298e7a7d891dc683e2984c0a2fa5d8c42c` fixes Fast VZ and QEMU compatibility
dashboard actions. Running entries showed enabled `창 열기`, but the bound Start action already
refused active runtime work. They now show disabled `실행 중`; accepted work also disables Start
through the existing admission predicate. Stop and backend ownership are unchanged.
Four new methods initially passed one and failed three with six assertion failures. The two-line
repair passed 11 native methods: four new, four lifecycle and three HVF action controls. The same
four new methods passed the actual shim with zero skips. These are unshown SwiftUI values and
direct closures; busy state is synthetic. No resource request, window reopening or VM is proved.

## Ubuntu creation requires the existing eligible template

Integrated `42e151d8892effa09a4dd560e2fd6f397b7315ec` removes arbitrary-first fallback only
for Ubuntu selection. Without a preferred Fast VZ direct-kernel template, existing guidance
appears and creation is disabled. Legacy nil boot mode remains eligible; ISO and QEMU creation
modes keep their fallback. Five unchanged regression methods passed three and failed two with
16 assertions; the one-line repair passed 14 native and the same 14 shim methods, zero skips.
The nine existing controls cover containment/alias admission and safe clone readback. The earlier
collision/cleanup description is retained with its correction. Unrendered sheet/footer values
and an explicitly owned tiny clone do not prove a UI create transaction, global-root write or guest.

## Sealing T12 inputs before admission

Integrated `d9700926a43982a173e17f28094ff074986a51b3` retains the T12 scenario and 20 lanes.
Three synthetic baseline methods failed: unsealed submission was published, a sealed manifest
was rejected because T12 was not a required-input tier, and unsealed execution reached a
forbidden firmware-builder stub. No real queue or guest was used for those reproductions.
The fixed 11-field manifest is required before publication; runner and pinned public firmware
are copied and hashed into unpublished staging. Running identity, ledger, copied inputs and
strict signature validity are checked before phases, without compilation, rebuilding, signing
mutation or ambient-toolchain fallback. Written/restored phases and old assertions remain.

Fifteen contract methods and adjacent sealed NVMe controls passed, with syntax, selected shell
and structural checks. The first shellcheck failure remains: state initialization moved into the
extracted receipt module without suppressing checks. Simulated 20/20 output is not hardware proof.
Manifest provenance is operator-attested, not derived from executable bytes; source/profile,
toolchain and Hypervisor-entitlement proof still belong to artifact preparation. Image and
initial-vars hashes describe fixed recipes. Observed initial vars are checked, but the shell
does not independently hash the complete in-memory image. Final vars remain a separate output.

## Certificate-positive preparation is separate from Accessibility trust

Fresh exact `5a846f8a` DEBUG host/launcher builds and release-boundary checks passed: seven parser
methods, DEBUG marker presence, ordinary-release marker absence and exact forbidden-release
rejection with nonzero status. Actual development-certificate signing succeeded for both apps.
Four selected-leaf checks passed across original/reconstructed bundles. Six post-sign roles and
eight complete bundle files matched; 60 proofs and all 5,069 source hashes were authenticated.
This supplies the previously missing certificate-positive observation, not launcher startup,
fixed dark rendering, UI action, guest behavior, notarization or release qualification.

The operator authorized Accessibility setup and completed OS authentication directly. The exact
signed driver was added and its settings toggle observed ON. That readback does not establish
actual launched-driver AX trust; no new live diagnostic result is claimed here. Original c2
Accessibility refusal and both dark-content mismatches remain failures. Private Windows media
and clean-Mac prerequisite work were explicitly skipped for this window; their criteria remain
OPEN. Exact5a artifacts are not automatically rebound to future source or validation results.

## Retained evidence and remaining validation

This table omits private paths; bound reports retain author mappings and failed iterations.

| Evidence | SHA-256 |
| --- | --- |
| Exact5a hosted reference failure | `22e1a5d4a5b20e1bf980a926c16f169e763892ac94fdcdf1afb51687843b4650` |
| Public-reference review | `795bc07e636ada234fbd1ee92a9ba28468ef15b71aa264c40b3889e22d7706b5` |
| Exact5a local full failure | `2423da53f38dcfc2a41e55a7f3378fc558eaeaed8ac23a8966e62b0244fa0687` |
| Exact5a macOS26 shim failure | `34838e4a77904ebe82b5e5bec3d3c73418a0d4f875441bd97ba29d364248812d` |
| Exact5a macOS15 shim failure | `9995ef0a05a6f9c5fe0a356d72878d3ceadaff906965aed76e4f0640566fa249` |
| Test-target routing report | `e9a079f638cbe6cccc295db4ea78f026acd8f9e04f0eec5b7fee0a4827e49ba7` |
| Test-target routing review | `52d52b806078d5bdfc1442c9a6be831ecbccc076f0cab025c999c6aa55ca4a44` |
| Console/block report | `cb2d802c52b8b7920dccdb91965929ea748f4da61dd0b0def92efff68b1e4e4a` |
| Console/block review | `6ff6e49c5621e61a6069c494d96a32c2608238bc76c38565cda58a9d80bcb9d5` |
| Generic action report | `996cfd8d253ea4b8b90525b29a24cddfc87607a2a1a3a55e7e0baf56b7ddb0ff` |
| Generic action review | `98aff15e595f207df08b6bd99a8e99d5f5623f0dfcb6a0d29d3c74034ea70f3f` |
| Ubuntu corrected report | `53a35605bf22ab7648ab6d80d126acb4c8d91e58736f8689476819df88fd2e0e` |
| Ubuntu review | `90f72802321822824e35d08f17fcb3ba8802e9c4ac36df6a1fd04fc6cbfda86f` |
| T12 implementation report | `930fc3823f625fc2dce7a3ba660ba3466a8b3a7d18821058457ac0a621cd0300` |
| T12 review | `20b0eb3749b1ad9eba503b5f69da7aa6ad5f5de5cada4155397ab781213ae412` |
| Exact5a certificate preparation | `1cd62ae1fa5c76a928a2239dd66866e553bcaf22a864c5f4c1d2c164b0bd2d2a` |
| Independent artifact authentication | `856b661739b18bd8745747f80bcc4156cca24a8fb65546e15c3ea94aac645814` |
| Exact5a permission-settings observation | `7878707b3978efb65b64f638c8a249b8cd003f2f8c41e033be8cc3bd530fb5d7` |
