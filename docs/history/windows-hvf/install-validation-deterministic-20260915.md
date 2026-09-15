# Install Start validation: deterministic evidence, 2026-09-15

Historical evidence record. The [capability registry](../../../capabilities/windows-hvf.json) owns current status and wording; this record changes no criterion and does not close A11.
Source commit `23d5ed5545dfd1d5f78baab678e138e5e52926ac` moves accepted Start validation to a detached worker using a Sendable plan and validator. Admission reserves visible validating/busy state synchronously.
The returned task acknowledges validation and pipeline scheduling, not installation completion. `session.cancel()` remains authoritative; cancellation of the acknowledgement handle does not cancel installation. Busy remains reserved until the validation decision settles, and retries cannot overlap it.

## Retained failure and native results

The baseline executed **1 test with 1 failed assertion**, exit code **1**, in **2.67 seconds**. Its actual injected Start validator ran on the main thread. This observed execution placement; it did not measure a rendered UI stall.
The baseline test source is preserved with SHA256 `8326446d7cb2f34831c5b9cb295fe27854605caae2fc4483f07c921f88349c0d`. The final corresponding test was adapted to await the new acknowledgement API; it is not claimed to be byte-identical. Additional progress/cancellation tests are new.
The final focused run passed **28 tests**, zero failures, in **5.19 seconds**. The expanded hosted product-flow filter plus `HvfWindowsBackendTests` and `LineAccumulator` passed **166 tests**, zero failures, in **31.32 seconds**, starting **2026-09-15 04:13:20 UTC**. Both exited **0**.
Five worker tests cover off-main validation, main-actor progress while a finite worker gate is held, immediate busy/duplicate refusal, cancellation before dispatch and during validation, error precedence, retry after acknowledgement, and acknowledgement-handle cancellation.
Existing store/admission tests now await accepted handles before cleanup, including unexpected duplicates and throwing metadata paths. Retained cancellation fixtures execute only already-cancelled pipeline closures' early guard and preserve their marker bytes.

Native receipts record base commit `b0d32d4bdaf5182fd887560fd0256157caf429a0` with the implementation in the working tree; sources and helper stayed unchanged during each run. All ten final Swift file hashes below match both passing receipts and committed contents at `23d5ed55`, without implying a full check of that later commit.
Product filenames are under `apps/macos/Sources/BridgeVMControl/HvfEngine/`; test/support filenames are under `apps/macos/Tests/BridgeVMControlTests/`.

| File | Lines | SHA256 |
| --- | ---: | --- |
| `HvfWindowsInstall.swift` | 257 | `0e0e82837bd5ce7189bd596eb6ada6b4fd0412cb0caf5d122df69e97b122681f` |
| `HvfWindowsInstallProgress.swift` | 40 | `dc130681ea196993731d4a00f46beaa7fca7d25839f046da62fc6e3e18616dce` |
| `HvfWindowsInstallSession.swift` | 221 | `c1a35cb17b2b475d93440c86c1fb1b47afd6c6893d991224c5f6c93156110176` |
| `HvfWindowsInstallValidationWorker.swift` | 9 | `4ae66fc13e6c9a5c2620eaa859d48a12fc5981b83cb117bedc8ece5849b2787c` |
| `HvfWindowsInstallView.swift` | 126 | `e9e148eaf304027922d46eb28ff7a194cca23edfbca2b48766791d0f57a04f8f` |
| `HvfRuntimeSessionStoreTests.swift` | 285 | `10411cec02cbc023c93b999c5ba06a97479e4cbc18de6a914aff549c1e073498` |
| `HvfWindowsInstallAdmissionTests.swift` | 120 | `07c0fb342a457568dd17d473353b3cca0a8d6e32f852a10f2e1bc998259f4b54` |
| `HvfWindowsInstallSessionStoreTests.swift` | 200 | `c8f7a6494f62dbd8fea4feb628907eb3b95edc5f1c22f013468f0c414a73d7a4` |
| `HvfWindowsInstallTestSupport.swift` | 125 | `a6a3a29088bb985232c7e4cbccabcaf6d6c86facd20b82c1f6d2727453222319` |
| `HvfWindowsInstallValidationWorkerTests.swift` | 137 | `6257aad89c01b18e8e03c9c6dbef5bbf41749e04e0a999163c3d11126e15eed9` |

Independent review found no introduced blocker and reproduced preservation of the validator/plan computation and existing cancel/run/process/finalization/timer/log bodies. The retained body SHA256 is `bc0cdae726f93cdd53180929769f3d684ceec737318bbfe7a3e2d215e2776143`; the extracted progress classifier remains unchanged.
The new tests use injected validators and captured jobs; they perform no actual installation, guest-media, VM, or WindowServer work. The cache label now reports stored-source presence pending verification. Plan construction, UI metadata reads and finalization remain separate responsiveness gaps; cancellation does not interrupt an in-flight synchronous validator. These results do not establish that all installation work is asynchronous or prove live guest behavior.

## Preceding checkpoint and remaining validation

The preceding full local check passed in **171.15 seconds**, exit code **0**, at HEAD `8e5b975dbadddd25ffccb94f4f1051b14fe80ac9`; its seal records unchanged checked contents committed as `b0d32d4b`.
At **2026-09-15 04:14:01 UTC**, all **78 hosted runs** for exact SHA `b0d32d4bdaf5182fd887560fd0256157caf429a0` had succeeded, including [CI 34927369769](https://github.com/Ketchio-dev/bridgevm/actions/runs/34927369769) and [Security and quality 34927369798](https://github.com/Ketchio-dev/bridgevm/actions/runs/34927369798).
At this observation point, `23d5ed55` still requires its full local and exact-SHA hosted checks. A11's final release regression seal remains required.

## Retained receipt identities

Names are relative to the operator's `hvf-continuation-12h-20260914` evidence collection; receipts/logs remain outside git. Validation receipts below share `campaign/install-validation/`.
Baseline log SHA256: `42afbcb352c043277322acb5d7a04b7a8a66b412a5ae810da0cfafa50d8de933`; focused log: `4eeb38257c2320c0adf0471c7f12702b6d4d29e66ecf6d62a9f6c60c5269c719`; expanded log: `ba7223b62ef9add8ba52887bedbac71f4e20e06a69d74dce55a969b9eddb0e21`.
Preceding full-check log SHA256: `acdc56717a1e5283824b82e3bf18c449c78a95f8b4111cd472477508f0bb0c2b`; existing native helper SHA256: `a51817734584797340fde03ef8afa6ad020272b3d73812a70505986c33518d1b`.

| Receipt | SHA256 |
| --- | --- |
| `baseline-observation.json` | `a15ecfd68113c2da80fb0451aabb707e5f0c5509746af4b7aeea046aaa7c6abe` |
| `validation-before.json` | `0b5edaa323bfbe8b2635a905eff03940a38d4d2ac0bbb65f23d71e267c6cdcfe` |
| `product-after.json` | `12d6f31af70fb549c2d2410872055324f7babda59f3a845b503c1924ebcb5822` |
| `validation-focused.json` | `8b8d1b7ad982aafc321c8273eb58c95cac2da8f11cc13f4c44fc5c4e414212a7` |
| `validation-expanded.json` | `18b09ceddb958df3a0254f578fed8de7ccb05924885d40f5e29c2a6763172982` |
| `local-check-seal-backend-configuration.json` (collection root) | `ef51203a76d1da02935a9026e23e9deeb968d57396ba3c42ac31a18a5d66187d` |
| `backend-configuration-hosted-later.json` (collection root) | `f83cd48f259fde9c6a954afdb095a250cfa9bb5fe1edae655a61cbfd01a93c6f` |

## Later full-check failure and portable test repair

The first full local check at `23d5ed55` started **2026-09-15 04:18:27 UTC** and failed after **130.11 seconds**, exit code **1**. Its only failing step was the repository XCTest shim suite: the shim does not provide `XCTestExpectation` or `fulfillment`. The failed receipt and log remain retained; the earlier native observations above are unchanged.
Repair commit `ce73620c8826d829e642750c91a90799b8cfe658` replaces test-entry coordination with a semaphore and a bounded **5-second** wait off MainActor. All three callers assert entry succeeded; the existing **10-second** worker gate, timeout, cancellation and retry assertions remain. The store fixture was extracted unchanged, and the acknowledgement-handle test was moved with only its entry-wait adaptation. Product source bytes are unchanged from `23d5ed55`.
With this repair, the expanded native filter passed **166 tests**, zero failures, in **33.52 seconds**, exit code **0**. The three repository shim suites completed in **50.24 seconds**, exit code **0**: **425 + 493 + 62 = 980 passed**, **0 failed**, and **1 skipped**. The skipped `HvfWindowsBootSeedTests.testSeedRealInstalledDiskWhenStaged` requires explicitly staged live-fixture paths; it is not counted as a pass. Shim results are separate from Apple XCTest results.
Both repair runs record base `23d5ed55` with the repair in the working tree, unchanged source/helper hashes during execution, and the same helper identity recorded above. The four test-file hashes below match both passing receipts and committed contents at `ce73620c`; this does not establish a full check of that later commit. At this later observation point, its full local and exact-SHA hosted checks remain pending. The scope and live-behavior limitations above still apply.

| Repaired or extracted test file | Lines | SHA256 |
| --- | ---: | --- |
| `HvfWindowsInstallTestSupport.swift` | 82 | `8fca39d95fa3fccf91572219096dfc94cdad8a12c955c61ecb5df60c9534e690` |
| `HvfWindowsInstallValidationWorkerTests.swift` | 111 | `babad0b199934d2e88e8d66dad3e1aaf5db98be1ed946cc84a508d0cb52c27cb` |
| `HvfWindowsInstallStoreFixture.swift` | 51 | `36f08903a97c9aae9fdcef7ce35ea0f79834da9b60f484f9772f7fb0f34f821f` |
| `HvfWindowsInstallValidationAcknowledgmentTests.swift` | 34 | `401baf2a9a876488b31ce9e7c83edbf06f3a3246f763a04bced81a0b9437527d` |

First failed full-check log SHA256: `e5da0c0528b06e8e98877c03d3af9801786803dcb7bbeba05a380f0d04440f4b`; repaired native log: `47cf1dc3bed49425db476aa6bdce3ef237c112c5e88c88a49938ebe2b060bc0c`; repaired shim log: `4ebfa834a1d764cb8663ed9685a189eb30c94b1ada0833f87977d21b8a4549ff`.

| Later receipt | SHA256 |
| --- | --- |
| `project-check-install-validation.json` (collection root) | `2f462637dd42198e62c670ec90b6afc3ea19e237a22d2b2e7961c54676e553a0` |
| `campaign/install-validation/portable-test-source.json` | `62b3b1c0db4599696cd578506a972010e18048dd0695683a50e3435eda3f90e1` |
| `campaign/install-validation/validation-portable-expanded.json` | `ecaa77508f1c0451d111917258e84a48b37a651196bf4b8f35c551ac3c99a88d` |
| `campaign/install-validation/validation-portable-shim.json` | `481750f2cffbcf7dd1872142d5c6d08eb10bf49cf0dba97b807c99cd2121eb4f` |
