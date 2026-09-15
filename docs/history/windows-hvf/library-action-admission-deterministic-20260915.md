# Library file-action admission: deterministic evidence, 2026-09-15

Historical evidence record. The [capability registry](../../../capabilities/windows-hvf.json) owns current status and wording; this record changes no criterion and does not close A11.
Source commit `bca8eea7e7919946fe7b7c703c38fd2523e33746` checks deletion, clone and move admission against current in-memory registration, existing action reservations, active retained install/runtime sessions, accepted cached generic work, and cached configuration coherence. Requests and final actions recheck before backend resolution or scheduling. Generic `running` alone remains an observation, not accepted work.
This packet is explicitly **one-way**: it refuses file actions after relevant app-owned work exists. It does not prevent a later Start after a file-action reservation.

## Retained failures and corrected result

The original native baseline executed **8 tests**, with **236 failed assertions across 7 failed cases**, exit code **1**, in **5.25 seconds**. Actual entrypoints admitted overlapping, active or stale requests, or failed to report refusal. The observed-running-only case passed. These failures establish admission defects, not actual data loss.
A separate intermediate-source regression executed **1 test**, with **18 failed assertions**, exit code **1**, in **4.82 seconds**: registration matched the request, but an older cached backend survived a busy reload and became idle without a second reload. Each of five entrypoints used its own fixture. The final memory-only cache check refuses that mismatch; ordinary idle reload can then rebind the model.
With the original two test files and separate cache test byte-identical to their respective failed runs, the expanded native filter passed **201 tests with zero failures**, exit code **0**, in **40.05 seconds**, starting **2026-09-15 05:27:55 UTC**. This comprises the previous 175-test filter, 9 new admission tests, and 17 `ControlModel` tests.
Coverage includes all nine ordered deletion/clone/move reservation pairs, request and final entrypoints, four nonstopped runtime states, validating and queued installation, generic busy/lifecycle/start-confirmation ownership, observed running alone, changed/removed registration, delayed confirmation, unrelated slugs and library roots, and stale cached backend recovery.
New fixtures use tiny owned registrations and counting fake backends. File-action jobs and installer pipeline jobs are captured and discarded without execution. Accepted install validation is acknowledged; the successful fake Start checks receive MainActor completion before cleanup. A failed acknowledgement timeout does not establish unconditional task quiescence; the fake has no file/process effects. No new test launches a runtime, executes a transfer, or opens a window. Broader existing tests retain their existing native helper use.

The native receipts record base `4b8e28dd7d2c6cf914f1a5b9899b3eb319e25566` with corresponding working-tree source, unchanged during each run. All nine final hashes below match the passing native receipt, final product receipt, and committed contents at `bca8eea7`; each test hash also matches its retained failed run. File identity does not establish a full check of the later commit.

| File | Lines | SHA256 |
| --- | ---: | --- |
| `apps/macos/Sources/BridgeVMControl/LibraryModel.swift` | 177 | `0493acd137d563e351b38923403ff646c10ff44a3440d26c8879e2167a014718` |
| `apps/macos/Sources/BridgeVMControl/LibraryModelActionRequests.swift` | 13 | `4cf931eb8823100e6848b0d817eb984ff4799613aab11ab98cc6f3998dd2b4e9` |
| `apps/macos/Sources/BridgeVMControl/LibraryModelRegistration.swift` | 14 | `217f3a0eaf68310ea303b852035dd07524e161fcbf2364358b48f50616ec858f` |
| `apps/macos/Sources/BridgeVMControl/LibraryModelActionAdmission.swift` | 38 | `b8b854c9209fad7fb8f40a6499106d97408752fe9e883eb8f30c4da9758d9c3b` |
| `apps/macos/Sources/BridgeVMControl/ControlModel.swift` | 297 | `36d49e0ba3eabdba5546e02ec3e32a1aaf4f9a632e40680b6f8253a869bbd353` |
| `apps/macos/Sources/BridgeVMControl/ControlModelCommands.swift` | 21 | `47d798e2811622f55649eb128d3e811078d44e08b5bccd816f1be7c40cba2d78` |
| `apps/macos/Tests/BridgeVMControlTests/HvfRuntimeLibraryActionAdmissionTests.swift` | 232 | `a6bf57c177165b86f5184546a6981fb736d9e85938118b094ec95f93bb0b5bda` |
| `apps/macos/Tests/BridgeVMControlTests/HvfRuntimeLibraryActionTestSupport.swift` | 243 | `285a502b78fd42683c2bba6d31048e68576356f353f4364039cef1e2f7825922` |
| `apps/macos/Tests/BridgeVMControlTests/HvfRuntimeLibraryActionCacheIdentityTests.swift` | 47 | `b601f4c266e600a2b2bbd9d5343fad4a0024102714b336f681360985264a8101` |

Independent review restored all three complete action methods to exact preceding source bytes by removing only the new admission guard and restoring the outer detached-task wrapper. The `add` method and both static command helpers were extracted unchanged. The query and reservation execute synchronously on MainActor; no cache creation, filesystem access or process probe occurs inside the query. Existing operation/completion bodies remain unchanged. No introduced source blocker was found.

## Preceding checkpoint and remaining validation

The preceding full local check passed in **164.72 seconds**, exit code **0**, at `bbd65548309a5d5776b821ad9336b95c14d42dd2`; the seal records unchanged checked contents committed as `4b8e28dd`.
At **2026-09-15 05:24:07 UTC**, that sealed SHA had **77 hosted successes and 1 failure**. [CI 34931591298](https://github.com/Ketchio-dev/bridgevm/actions/runs/34931591298) and [Security and quality 34931591064](https://github.com/Ketchio-dev/bridgevm/actions/runs/34931591064) succeeded. [Inventory contracts 34931591285](https://github.com/Ketchio-dev/bridgevm/actions/runs/34931591285), attempt 1, failed its PowerShell 7 child-process `WaitForExit(10000)` deadline before output assertions. The audit retains the job log and exact checked-out sources; no wire-format assertion failure or specific cause was established. The deadline was not changed.
One authorized, unchanged failed-jobs retry was still in progress at **2026-09-15 05:28:54 UTC**: attempt **2**, [job 104264607643](https://github.com/Ketchio-dev/bridgevm/actions/runs/34931591285/job/104264607643). Attempt 1 and its failed hosted observation remain preserved; this snapshot does not establish all hosted checks green.
The later **2026-09-15 05:31:04 UTC** observation records **78 hosted successes** for `4b8e28dd`. Attempt 2 succeeded with the same 10-second deadline; its final metadata and log are retained. This unchanged retry does not identify the first failure's cause or erase that failure.
At this observation point, `bca8eea7` still requires its full local and exact-SHA hosted checks. This packet proves no reciprocal Start exclusion, coordination across library instances or external actors, physical-path isolation, external metadata discovery before reload, operation completion/rollback, rendered alerts, live data safety, or guest behavior. A11's release regression seal remains required.

## Retained receipt identities

All receipt paths below are relative to the operator's `hvf-continuation-12h-20260914` evidence collection and remain outside git.
Native log SHA256: original baseline `9d5ca731a451699b1b5d91b1dc262501e3cec18e5bde265f27da64e90d23ccbf`; cache baseline `a2b8ebe5ec69812a076ff6e13e8ed922c0d61feb8605777edf097e9e0f2857b2`; expanded pass `68a4391541dae3ff8543070e82437ed6b1fc9ec862f53a345aa719d0ab6fa058`.
Native helper SHA256: `a51817734584797340fde03ef8afa6ad020272b3d73812a70505986c33518d1b`; preceding full-check log: `305cdcc0e79535bbc846ba3d73e3fcd3eb72b6733e253c8ecaa6c0a7f65efef0`; failed hosted job log: `900a0dae9882ae4ce188dd4128b03560ac1403d996af2fdd9ad746fb31a74685`.
Successful unchanged retry log SHA256: `54e352abfe9365c2c971720c639bb79c1b77d9477723512757a953d476d709e2`; final retry metadata: `0e03e66784d6600a59d43473b8f97c547e68dedd6c1973a42869d1d98714b92b`.

| Receipt | SHA256 |
| --- | --- |
| `campaign/library-action-admission/action-admission-before.json` | `0b3914cee4075b0a39ad730493047dfe8da8d69f7c3898d0245c2a83b49fea56` |
| `campaign/library-action-admission/cache-identity-before.json` | `e17d1abb93ecee3861e0702e3b4d092f042a3b28b830334fe75c4e2ec8c2c79c` |
| `campaign/library-action-admission/action-admission-after-expanded.json` | `da18108606e1eec5b653a2528d8123d4a2525f8b5298093c4760dfd7289b7117` |
| `campaign/library-action-admission/product-after.json` | `8cf7b42aca4e6a802243a4f28d8af89a6a907bc4007f9a199988c7cf799bd8b6` |
| `campaign/library-action-admission/exact-extractions.json` | `5cf9f67924e278f8bb235e904d1064b91fff3b8ef4d521b7ca8f507e9013db7d` |
| `campaign/library-action-admission/normalized-preservation-final.json` | `1c05dcc614522d1e4727d71426ddad16d0e4fc71e5b219da2fac6a89aa501ee1` |
| `local-check-seal-palette.json` | `4144a5685fd50cd99bbbb223c228aeb0e377ba44293d77d4ecf5f576b8ff7e56` |
| `campaign/palette-ci-failure/first-failed-palette-hosted-later.json` | `a30b15a7b16436a781b046146522a8a0475bd28f7a7515f3d0ca9920a524f0b1` |
| `campaign/palette-ci-failure/failed-attempt1-receipt.json` | `17aa7d3504c03295ed200a57e2226d40dcc4643a65c4cb575d362900146311b9` |
| `campaign/palette-ci-failure/rerun-one-snapshot-receipt.json` | `866c1e94a67004bc9c13473cb96733ecae3f9a29787c0603cd4fd9b5c7a899ec` |
| `campaign/palette-ci-failure/parent-attempt2-receipt.json` | `1563ebca29e8a4e4797aa4f027652f6079798a23f4f71660c53cf18d910f7149` |
| `palette-hosted-after-retry.json` | `b159558d0ac2e29b7c510ff5bc990063694cae3c37a67ab746bd4ac9f9148d8d` |
