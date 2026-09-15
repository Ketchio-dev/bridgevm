# Library work admission: deterministic evidence, 2026-09-15

Historical evidence record. The [capability registry](../../../capabilities/windows-hvf.json) owns current status and wording; this record changes no criterion and does not close A11.
Source commit `408ab3c32da1bcf7a07f9a1f442d53b015256ddf` adds the reciprocal side of [file-action admission](library-action-admission-deterministic-20260915.md): bound runtime launch-configuration acceptance, Start and attachment, install Start, and generic Start/resource Apply refuse conflicting or obsolete work in the same `LibraryModel`.
The synchronous MainActor query checks current in-memory registration, exact cached object and immutable accepted source configuration, file-action reservations, active installation/runtime, and accepted generic work. Generic `running` alone remains observational. Weak owner expiry refuses; nil callbacks retain standalone behavior. Silent automatic attachment still checks admission. Existing eligibility guards remain first, and eligible refusal leaves operation state unchanged while explicit requests publish `operationError`.

## Retained failures and corrected result

The initial compiled trial executed **15 tests with 312 failures across 14 failed cases**, exit code **1**, in **6.47 seconds**. Uppercase fixture IDs disagreed with persisted lowercase registrations, invalidating **26 reservation prerequisites**. This is preserved fixture-failure evidence, not a clean product baseline. Only fixture ID normalization and explicit saved/scanned identity assertions changed before a new snapshot; the other five new test files and API-only product baseline stayed unchanged.
The corrected API-only baseline executed **15 tests with 343 failed assertions across 13 failed cases**, exit code **1**, in **2.56 seconds**. Reservation, path/helper, identity, key-call, acknowledgement and gate prerequisites passed. Conflicting, stale or expired-owner entrypoints admitted work or omitted the required notice; standalone and independent root/slug cases passed.
The first expanded run executed **216 tests with 4 reported failures in one existing routing case**, exit code **1**, in **42.30 seconds**; all 15 new admission tests passed. Latest metadata incorrectly replaced a busy generic control panel with a new HVF view. The correction adds a detail-only eligibility guard before the latest-resolving accessor, retaining active-runtime precedence and the generic panel's accepted configuration. Direct stale getters still resolve current metadata.
The final expanded native run passed **216 tests with zero failures**, exit code **0**, in **39.53 seconds**, starting **2026-09-15 06:18:14 UTC**. The six canonical test files are byte-identical to the corrected failed baseline. The existing routing test is unchanged across that baseline and both expanded runs; all 160 native test files stayed unchanged after the expanded routing failure. Durations above are receipt wall times, including invocation overhead.
Coverage includes all three file reservations, named entrypoints, active/self ownership, immutable source versus edited launch options, stale/replaced/removed/recreated registrations, generic confirmation ownership versus observed running, independent roots/slugs, expired weak owners, nil standalone callbacks, and install callback assignment/rebinding. Actual detail values establish route and object identity without rendering a window.
New fixtures use tiny owned registrations and helper markers; helper resolution and hashes are checked, but markers are never executed. Accepted runtime Start calls reach absent-owned-resource readiness failure before launch/preparation; counting false process lookups and throwing key-provider sentinels perform no runtime process or key operation. Injected validators and counting fake backend methods execute. File-action and installer-pipeline jobs are captured and never run. Accepted asynchronous work in passing tests is acknowledged before cleanup; a failed acknowledgement retains the fixture and proves no settlement. Experimental singleton checks observe identity and nil binding only. Broader existing tests retain their native helper use.

The native receipts identify base `0aa7908528f355ccf388bbe4e19eebb59217b07f` plus working-tree source unchanged during each run. All 20 hashes/counts below match the final product receipt, passing native source map and committed contents at `408ab3c3`. Source identity does not establish a full project check of that later commit.

| File | Lines | SHA256 |
| --- | ---: | --- |
| `apps/macos/Sources/BridgeVMControl/LibraryModel.swift` | 177 | `26f2886941fe450683dc9cc6f3bb8f4b878b92c0193fa573c5c93c8260e4c4cb` |
| `apps/macos/Sources/BridgeVMControl/LibraryModelWindowsInstall.swift` | 48 | `cec63a41cff7d5b57f63ce4520fe03ea5e61888015a6ceb07471baaf6baad2eb` |
| `apps/macos/Sources/BridgeVMControl/LibraryModelHvfRuntime.swift` | 54 | `e52da88baa56303f952e671ec0dc5831dda25b47b53e52230c8cd8d44f7b0ae4` |
| `apps/macos/Sources/BridgeVMControl/HvfEngine/HvfEngineSession.swift` | 537 | `889b4bed058f2b9f40d94d13248f1b7a305b2a2b6749b07532fba1cb52a31117` |
| `apps/macos/Sources/BridgeVMControl/HvfEngine/HvfWindowsInstallSession.swift` | 221 | `4282760dc5de060f6c8a0c3ece73cc5170513d725a90ba697f01533a3e0d01ee` |
| `apps/macos/Sources/BridgeVMControl/ControlModel.swift` | 254 | `4f0bc001aae9771a116444d0f3a67714ee207023cef579583094f7f9962ab35d` |
| `apps/macos/Sources/BridgeVMControl/ContentView.swift` | 491 | `d5aabd9935f4862f6de24750a4d8eb527137f8a59f7f6ad859553bcfb7679067` |
| `apps/macos/Sources/BridgeVMControl/LibraryModelWorkAdmission.swift` | 52 | `b00de3d63edc2dc3a8e3f5cd0f89220d6303f0428c16149a55c5f692ad8a2493` |
| `apps/macos/Sources/BridgeVMControl/LibraryModelSessionBindings.swift` | 56 | `58c746985bef1e0389f9ce79762efb16edc319f9b7a3df9c1f36a413fe58bebd` |
| `apps/macos/Sources/BridgeVMControl/LibraryOperationAlerts.swift` | 53 | `1d543201dc0fa7da6b6357a6e70a4f515195b25f23db89914c2e6b80941a0d21` |
| `apps/macos/Sources/BridgeVMControl/ControlModelGuestCommands.swift` | 50 | `3c6c9f11b9e28969ad2bd808fcaee3c57b0d88f0af9454b84fa1c5585d3784f8` |
| `apps/macos/Sources/BridgeVMControl/HvfEngine/HvfEngineSessionAdmission.swift` | 16 | `82011ca0cba2038e7b6a3dc4a427f83dbc6e3c9e48f35e3da91d88511e6b5eb9` |
| `apps/macos/Sources/BridgeVMControl/HvfEngine/HvfWindowsInstallLifecycle.swift` | 18 | `42867b95b01d6bb96c158582b79ebe4cfdfb557369439f054e89b4f7bbd2db25` |
| `apps/macos/Sources/BridgeVMControl/LibraryDetailView.swift` | 40 | `888cd2a03f10b3afa14f023afe3e7993c4ea2a4791b3a94d7660701b8ea015c4` |
| `apps/macos/Tests/BridgeVMControlTests/HvfRuntimeWorkAdmissionBackend.swift` | 111 | `43aa3a27eba2206d311b6fc184ff6a680f946d0775c4cf2f1c679332e9ce95f1` |
| `apps/macos/Tests/BridgeVMControlTests/HvfRuntimeWorkAdmissionFixture.swift` | 250 | `5429e68f51db39400449f074d18f78edc59e8461135e8d3beb0affab607f0cb5` |
| `apps/macos/Tests/BridgeVMControlTests/HvfRuntimeWorkAdmissionIdentityTests.swift` | 156 | `835e75d0b057ed2dd77a76cb4f6fdfb4ab610b3381b588ac6236fd016a0ef570` |
| `apps/macos/Tests/BridgeVMControlTests/HvfRuntimeWorkAdmissionLifecycleTests.swift` | 172 | `9d898cb5330ab46036b4641cac1e827c169383e371b7e4753dbaf286cf303f64` |
| `apps/macos/Tests/BridgeVMControlTests/HvfRuntimeWorkAdmissionOwnershipTests.swift` | 194 | `e34b07a1b6501dcc2c3631c244a463d5613b02e8a4ffad915ffa78060cf73a3f` |
| `apps/macos/Tests/BridgeVMControlTests/HvfRuntimeWorkAdmissionReservationTests.swift` | 128 | `f9dbc3abbb1d6f47175fc396fdd81be0400f674a4ef92d217ea5a529e6fbacda` |

Independent review restored all seven original product files exactly after reversing only approved changes. Baseline extractions preserve runtime helpers/accessors, generic guest commands and the three existing alerts; the documented normalizations are the existing `rootURL` getter replacing private `libraryRoot`, a defaulted install `repoRoot` parameter/pass-through, and two trailing separator lines. The final guards, bindings, full install source snapshot and lifecycle holder are explicit changes. Existing file-action/completion, Stop/Cancel, process/polling, validator, pipeline and finalization bodies remain unchanged. Removing the later detail wrapper restores the preceding binding file; its caller differs only by accessor name. No introduced source blocker remained after routing review.

## Preceding checkpoint and remaining validation

The preceding full local check passed in **159.77 seconds**, exit code **0**, at `bca8eea7e7919946fe7b7c703c38fd2523e33746`; its seal records unchanged checked contents committed as `0aa79085`. At **2026-09-15 05:57:02 UTC**, that sealed SHA had **78 hosted successes**, including [CI 34933886306](https://github.com/Ketchio-dev/bridgevm/actions/runs/34933886306) and [Security and quality 34933886231](https://github.com/Ketchio-dev/bridgevm/actions/runs/34933886231).
At this observation point, `408ab3c3` still requires its full local and exact-SHA hosted checks. The result covers only the named same-library entrypoints. It proves no external actor/other-instance coordination, physical-path isolation, external metadata discovery before reload, missing-registration control navigation, rendered alerts, actual file-operation/installer completion or rollback, live guest survival, or release criterion. Stop/Cancel, guest commands, snapshot/vTPM commands and other entrypoints gain no new admission guarantee. A11's release regression seal remains required.

## Retained receipt identities

All receipt paths below are relative to the operator's `hvf-continuation-12h-20260914` evidence collection and remain outside git.
Native log SHA256: invalid fixture trial `d3b0aae98e6398529cd363743e7210b4558f5c891e6df8b88d58d85efb8b408b`; corrected baseline `2984574f1f6ad83a00639bafb8b39dc005f7e2b6a628c916b2dafc2cceca32f0`; expanded routing failure `1a8f6e0ff750d8fb423a4daee02ef144e500b82f3f78593c5ad422b936b9878d`; final expanded pass `d55eaf698d50111ddbf04ceea503e28b680b05c786223faa2d90e6abc360e643`.
Native helper SHA256: `a51817734584797340fde03ef8afa6ad020272b3d73812a70505986c33518d1b`; preceding full-check log: `cbc2b63cb8de64ada9f82cb071674574ad0c95d23142c335addf3a9d4ff00af4`.

| Receipt | SHA256 |
| --- | --- |
| `campaign/reciprocal-admission/work-admission-before.json` | `98577b3ee8634eb570e9a45ba0e20321ec17d3ad84ed34b3e29af946249ebef4` |
| `campaign/reciprocal-admission/initial-fixture-trial-observation.json` | `aab74e7da65eedeb33bffe0f2431a34b3044e1e07161056a63df3aee304527b9` |
| `campaign/reciprocal-admission/prepared-tests-canonical-id.json` | `7a760c8220278157a5ba96610b0406853b4b5deae10f7296c58cce75b0e4290c` |
| `campaign/reciprocal-admission/work-admission-before-canonical-id.json` | `eee766ee5be4b36e2d590a57879333efc19b7cf58f5e472ebd091a07ee020255` |
| `campaign/reciprocal-admission/baseline-observation.json` | `bac6a7ed10516bbf2189ab41a2594cef00b8aa60d3fcd3140c4d1d8e29fd9b2b` |
| `campaign/reciprocal-admission/work-admission-after-expanded.json` | `3ed7431e5fe3d4d8ac34164feeb74bd9d5cdbaca08374a83af9c46ea6ec9b3a9` |
| `campaign/reciprocal-admission/expanded-routing-regression.json` | `7341ac8450c1c8b66e6e3ee15afc0a0025ce0e29d5c2f61742d605fcfa39d520` |
| `campaign/reciprocal-admission/work-admission-after-routing-expanded.json` | `f91461a23bae1a29682639fe9c1525d38bfd9a74f896d3d07a17ebfce5a12507` |
| `campaign/reciprocal-admission/baseline-extraction-proof.json` | `121c29ec0559aeed36e42fefc8b1c3cadc55b27228de0f429a0e3e0657503a67` |
| `campaign/reciprocal-admission/final-preservation-proof.json` | `609215f4a2e71355386e58586806ceecef4ee08b7417079135f8c520b1ce812f` |
| `campaign/reciprocal-admission/product-after-routing.json` | `fa7f90e4450000bfe814e7c474abbf7bc2866a6c9d70404b3a3c6d5bb356883a` |
| `campaign/reciprocal-admission/final-preservation-routing-proof.json` | `92c2ca6f9bfc7dfe386fc1e674db9c29706a3b27adfc75d39d3fd09e5a4086a7` |
| `local-check-seal-action-admission.json` | `dcaf74338ba4c27fe92ba0c29e2782f3c82934bb61dfc7cb2bc5a33c4035be62` |
| `action-admission-hosted-final.json` | `953355e88facbaea256864ffa6b8e7606b10743858a2a151240aa6f0cc91f1c7` |
