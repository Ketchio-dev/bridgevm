# Retained controls: deterministic evidence, 2026-09-15

Historical evidence record. The [capability registry](../../../capabilities/windows-hvf.json) owns current status and wording; this record changes no criterion and does not close A11.
Source commit `6b318e8090cea18aacbe818f143c5d9c807c2543` retains existing active own-HVF runtime/install controls when a library reload observes their registration missing. Capture precedes ordinary store reconciliation and reads accepted session records without creating a backend/session or saving a registration. Exact objects and accepted display metadata remain available through terminal state until explicit terminal-only dismissal.
Opaque tokens distinguish session kind and object identity. Same-slug re-registration and a later replacement object's removal do not overwrite older records. A disappearing selected ordinary slug chooses an active matching capture, with installation first; retained selection survives Pro mode and terminal transitions. First-run keeps its priority. Dedicated views expose existing status, Stop/Cancel and terminal dismissal; they provide no Start, attachment, editor or registration-repair path.

## Retained baseline and corrected result

The primary native baseline executed **1 method with 16 table cases and 32 failed assertions**, exit code **1**, in **6.48 seconds**. The scaffold already contained the real index and view APIs, but reload capture was absent and existing selection fallback was unchanged. Actual detail construction lacked a retained child in every case; Pro won eight cases, and ordinary runtime fallback created an extra generic model and runtime in four. Fixture identity, owned-path/access prerequisites and regular-file snapshots passed. No installation validator or pipeline job ran in this primary method.
The missing-token guard skipped record-specific, repeated-reload, terminal and notification assertions after the missing child. The other ten new methods compiled but did not execute in this baseline. It therefore demonstrates the missing route, not failed coverage of all downstream contracts.
After capture and retained selection integration, the expanded native filter passed **227 tests with zero failures**, exit code **0**, in **39.82 seconds**, starting **2026-09-15 06:48:13 UTC**. This includes all **11 new methods plus the prior 216 tests**. All four new test files are byte-identical to the primary failed run; all 160 prior native test files remain unchanged. Durations are receipt wall times, including invocation overhead.
Coverage includes four nonstopped runtime states, last/remaining registration rows, Pro/sentinel navigation, validating and queued installation, same-slug runtime/install identities, terminal retention after ordinary pruning, selective dismissal, active reappearance and second-object removal, weak ownership, subscription cancellation, and state-only parent notifications. Synchronous `Published` callbacks observe old state without pruning or rerouting; events/heartbeat/install logs remain local to the observed control view.
New tests use tiny owned registrations and helper markers. Runtime states are assigned as observations: no runtime Start, attachment or Stop executes. Existing Stop button wiring is checked statically. Actual install Cancel runs against a finite injected validator; accepted validation is released and acknowledged on normal and throwing paths. Installer pipeline jobs are captured and never executed, including queued cancellation that remains active. No new test runs a VM, helper, file action or rendered window. Broader existing tests retain their documented native helper use.

The native receipts identify base `3ea26b8ebb5b19a9c6a130605a19d5e05c5431f5` plus working-tree source unchanged during each run. All 19 hashes/counts below match the passing native source map, final product receipt and committed contents at `6b318e80`; the four test hashes also match their prepared snapshot and primary baseline. This source identity does not establish a full project check of the later commit.

| File | Lines | SHA256 |
| --- | ---: | --- |
| `apps/macos/Sources/BridgeVMControl/ContentView.swift` | 366 | `1ee952831ef37a939b74a5c0b81069f53b5644a0f3768f02bf80d1e3d49b6b78` |
| `apps/macos/Sources/BridgeVMControl/HvfAcceptedSessionRecords.swift` | 23 | `857b7dc85e26ced95a680b95035b674aaab1abb2208f148da1e1c4914a850c1f` |
| `apps/macos/Sources/BridgeVMControl/LibraryDetailView.swift` | 33 | `a30cea9eb77f97cca3bdaaf33296c0af6e37c14fdc096c5b2a790b07dfa18046` |
| `apps/macos/Sources/BridgeVMControl/LibraryEmptyState.swift` | 15 | `be4e85161c4cc0ecd6b4378166296d7b19ae8977d032a1e48f2bf3c23ea5b893` |
| `apps/macos/Sources/BridgeVMControl/LibraryModel.swift` | 177 | `8e36a687eb4f832314a6821332fcf9b2e5e341d2c52562c85f283cf5b67693d0` |
| `apps/macos/Sources/BridgeVMControl/LibraryModelHvfRuntime.swift` | 54 | `45b10d7dd5ac073e17eeb3583e28806e7a7c5819e068b1bb4477704f4eaf32be` |
| `apps/macos/Sources/BridgeVMControl/LibraryModelRetainedControls.swift` | 47 | `df6ee1c544edcc1c46acd311598b0673b6bf2209c33de2a3d3c5505fa2021583` |
| `apps/macos/Sources/BridgeVMControl/LibraryModelWindowsInstall.swift` | 47 | `d10f3e20c9caf5b3dd700aa6d37f91bccfaeed67c9457dbaeed2f9dc15411be9` |
| `apps/macos/Sources/BridgeVMControl/LibraryRetainedControlDescriptor.swift` | 76 | `b59d4152495193034eafadfa0f5e9644018d73bcf9ec02864723b3459e423dbd` |
| `apps/macos/Sources/BridgeVMControl/LibraryRetainedControlStore.swift` | 34 | `48657e098e4092b0145c15c7fd07f02911f35e399e316f16902fd94db1ce4715` |
| `apps/macos/Sources/BridgeVMControl/LibrarySidebar.swift` | 130 | `ed56cd2e625dfc4ba38c3b08ead96c85197c1fe71e2324323ce750c7fd34160b` |
| `apps/macos/Sources/BridgeVMControl/RetainedControlDetailView.swift` | 19 | `8e2062d4a198bb03fb936c1c7a019f2e3ee9a11eb266e60083d4a7b66d457f1e` |
| `apps/macos/Sources/BridgeVMControl/RetainedControlsSidebarEntry.swift` | 21 | `f0fa0a11297d07105a7c8e1fb55d21bd9fd758764d58dabdf35cad5ae9db25c7` |
| `apps/macos/Sources/BridgeVMControl/RetainedInstallControlView.swift` | 31 | `38e4bf7c80dac2a25a260de4dcbd9273f1adbeca9850adedff475b2bdb466bca` |
| `apps/macos/Sources/BridgeVMControl/RetainedRuntimeControlView.swift` | 31 | `a62e01163216e81d31cf044403b5a0afdc365d6171902702f54587d19124065d` |
| `apps/macos/Tests/BridgeVMControlTests/HvfRuntimeRetainedControlFixture.swift` | 116 | `8bbf6637cb842d6e42a4ca6ca224a9c60ce523e5c192651430d6224cb1f9d34d` |
| `apps/macos/Tests/BridgeVMControlTests/HvfRuntimeRetainedControlLifecycleTests.swift` | 204 | `50fea5cee3e1f43bd853f2ab9e8c7b85d49fb9210db6744338d0ace2fe022841` |
| `apps/macos/Tests/BridgeVMControlTests/HvfRuntimeRetainedControlNotificationTests.swift` | 141 | `ddff874b18afa1de5b3f11d961cffda9dc80e6fd86e9e094f6792232bcf85923` |
| `apps/macos/Tests/BridgeVMControlTests/HvfRuntimeRetainedControlRoutingTests.swift` | 195 | `c44de07f34ffc1ead34c57a60d6ab73dfa721db1b47e9cd6df94ac434f571f8f` |

The extraction proof restores all five original product files after only approved changes: immutable Entry record extraction/type-name normalization preserves private caches and store policy; the complete sidebar is restored after import/new-component/EOF normalization; the empty-state body is exact after property-header/struct-wrapper normalization; initial and missing-selection statements are delegated unchanged in the baseline. Existing file-action and reciprocal admission bodies remain unchanged. The independent final proof removes only the new capture line and exact retained-selection preamble/helper, restoring both complete phase1 files; the other 13 product paths are byte-identical to phase1. Existing ceilings were respected. Final source review found no introduced blocker.

## Preceding checkpoint and remaining validation

The preceding full local check passed in **159.95 seconds**, exit code **0**, at `408ab3c32da1bcf7a07f9a1f442d53b015256ddf`; the seal records unchanged checked contents committed as `3ea26b8e`. At **2026-09-15 06:41:14 UTC**, that sealed SHA had **78 hosted successes**, including [CI 34937196001](https://github.com/Ketchio-dev/bridgevm/actions/runs/34937196001) and [Security and quality 34937196070](https://github.com/Ketchio-dev/bridgevm/actions/runs/34937196070).
At this observation point, `6b318e80` still requires its full local and exact-SHA hosted checks. The result establishes in-memory identity/lifetime, actual UI-value routing, notification and explicit terminal dismissal. It proves no rendered interaction/button dispatch, real process Stop/cancellation completion, guest survival, registration repair, durable history, external actor/path coordination, filesystem watching or missing generic-control recovery. Capture requires an active session when reload observes the missing row. A11/A19 remain open; no live criterion is promoted.

## Retained receipt identities

All receipt paths below are relative to the operator's `hvf-continuation-12h-20260914` evidence collection and remain outside git.
Native log SHA256: primary baseline `5c66d809f32e18b3d667b14b4112101966e50c7620b42a4c0cefe4d7f7ece267`; expanded pass `49bda8990e06ce8741031229f5aac69cdc779ccc1afcd23ef280cb7e7bf2e597`.
Native helper SHA256: `a51817734584797340fde03ef8afa6ad020272b3d73812a70505986c33518d1b`; preceding full-check log: `b7220b2db217821d459681b9ef2430e1bd892d67ef658e6862c41e4a38e30a1c`.

| Receipt | SHA256 |
| --- | --- |
| `campaign/retained-controls/retained-controls-before-primary.json` | `d8960090ba5f8964452d5feb1d56f6cd4a43729fde5bcc17d7bfcc470805c9ea` |
| `campaign/retained-controls/baseline-observation.json` | `790f2e2b0bdcf39c22f28a77c856dbfb70e21313d649c55aeed2c4968fd2f6b0` |
| `campaign/retained-controls/prepared-tests.json` | `e87363648b770d45b74bd07b3e154e4f36eaba4a8f30c96ad851635e7e03213b` |
| `campaign/retained-controls/retained-controls-after-expanded.json` | `15d523d358180d2bc4da480324bdac23197e07bb036e24f7ecd4c82437053e18` |
| `campaign/retained-controls/product-baseline.json` | `bc7227358d09ba1cf74e6b67830c8508f3cc93cb0134f11a73499ba2d6797bb2` |
| `campaign/retained-controls/baseline-extraction-proof.json` | `22aac302c669957a0e07f517fa38039a7fa33a928990a0c6aa7156fc4aef8686` |
| `campaign/retained-controls/product-after.json` | `4346b90ad14aac48e424fcc8e12bf6b434666e64cdce72e1fa44886a011ff722` |
| `campaign/retained-controls/final-proof.json` | `d1a400daa9c466d17962329cd3d1c9561ac0e4923b0046b01b4c06d306f9327b` |
| `campaign/retained-controls/prior-tests.json` | `d5c25a55bc2f204a327aed54fe268c9529bdec87954ecec89e27d62af3cb3369` |
| `local-check-seal-work-admission.json` | `f322df4ff82391f6eddde6c0c12eefe6dd21f897645a9878e4e45c85a74a50aa` |
| `work-admission-hosted-final.json` | `04dd17769d55b89dc52fb75e42d25c73dea49eb80a6814c6319962c216b35115` |
