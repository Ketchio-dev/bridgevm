# Install-plan preparation: deterministic evidence, 2026-09-15

Historical evidence record. The [capability registry](../../../capabilities/windows-hvf.json) owns current status and wording; this record changes no criterion and does not close A11/A19.
Source commit `b941ae18a834647c0fd5b43bfc94eadcb9ee01e9` moves actual pending-detail Plan construction to a detached worker with immutable configuration, request and root inputs. The ready host observes an explicitly injected session from the existing authoritative cache. An active accepted install wins before current eligibility or request reads; unchanged idle reuse retains its existing configuration-plus-request policy, including a compatible session created through the synchronous compatibility accessor. Accepted Plans remain immutable, and Start validation and install process/cancel/finalization bodies are unchanged.
Ordinary view lookup never publishes into an existing observed preparation handle: equal pending/failed inputs or an identical ready session reuse it; changed lookup results create a fresh initialized handle in a private index. Explicit retry updates only the currently indexed failed handle. Completion checks attempt UUID, index identity, current registration/request and expected cached object; competing active/compatible sessions win. Weak ownership crosses the worker await. Preparation performs reads only and reserves no media; small request JSON loading remains synchronous. Full ISO hashing on this path applies to unsealed requests; normally created requests already carry a staged ISO SHA.

## Retained baseline and corrected result

The first native attempt failed compilation, exit code **1**, in **5.92 seconds**; no test executed. A new test incremented optional `VMConfig.memMiB` without unwrapping it. The retained repair uses `try XCTUnwrap` before adding 1024, preserving the resource-change case and its assertions. That compiler failure is not the behavioral baseline.
The compiled primary baseline executed **1 method with 2 sealed/unsealed cases and exactly 2 failed assertions**, exit code **1**, in **2.78 seconds**. The actual pending-detail route invoked the guarded real Plan builder synchronously on MainActor before initializing its fresh ready handle. Both failures were the required off-main assertion observing `Thread.isMainThread == true`; every other primary prerequisite and owned-byte check passed. The other nine new methods compiled but did not execute, so this run establishes execution venue only.
After the two-file worker/coordinator scheduling correction, the expanded native filter passed **237 tests with zero failures**, exit code **0**, in **40.27 seconds**, starting **2026-09-15 07:32:17 UTC**. This includes all **10 new methods and the prior 227 tests**. All nine affected test/support files equal their compiled-baseline snapshots; 161 other prior test files remain byte unchanged. Durations are receipt wall times including invocation overhead.
Coverage uses the actual detail and ready-host body values, guarded real Plan construction on tiny sealed/unsealed ISO inputs, finite worker gates and a separate MainActor acknowledgment. It checks navigation coalescing, active/idle cache precedence, current request edits without reload, changed/missing registration and A→B→A attempts, synchronous-cache publication races, same-index explicit retry, independent owners/roots, weak owner release, and zero existing-handle notifications during body-time lookup. Existing runtime/install identity and routing assertions are preserved with explicit asynchronous setup; negative route checks include the new preparation host.
Owned regular executable helper markers satisfy resolver selection but are never executed. Most new cases perform no Start; the authorized active-install fixtures use real install admission with finite injected validation and captured, never-executed pipeline jobs. Tracked gates and acknowledgments settle before owned inputs are removed, including throwing paths. Unexpected host-discovery failures retain fixture inputs when an acknowledgment cannot be obtained; those failures cannot count as passing coverage. No new test runs a VM, runtime Start/attach, helper, file action or rendered window. Broader existing tests retain their documented native helper use.

Native receipts identify base `b22abaa70d43ea0bc9c56cb509c26977fde54b8a` plus unchanged working-tree source during each run. All 21 source/test hashes and counts below match the passing native source map, final scope receipt and committed contents at `b941ae18`; all nine test hashes also match the compiled primary baseline. This identifies the measured source and does not substitute for a full check of the later commit.

| File | Lines | SHA256 |
| --- | ---: | --- |
| `apps/macos/Sources/BridgeVMControl/HvfEngine/HvfWindowsInstallPlanWorker.swift` | 39 | `a3a42868d67ac4772264299c700ccc0e6df7a106315884a649ab9c8c3be26142` |
| `apps/macos/Sources/BridgeVMControl/HvfEngine/HvfWindowsInstallPreparation.swift` | 65 | `ee6792ad69c41f57a16f2849ab8f7990389765c3ed17f51dcf623b666acead44` |
| `apps/macos/Sources/BridgeVMControl/HvfEngine/HvfWindowsInstallPreparationView.swift` | 35 | `6eec50473f2a10c9cb86b3d556739ff88d177cfb96037aeb26af711f29db7b1f` |
| `apps/macos/Sources/BridgeVMControl/HvfEngine/HvfWindowsInstallView.swift` | 126 | `6e8cbd2057ee3841e2a005ad6238a55455d5f99433a56a55ad6bfaf30b649ee4` |
| `apps/macos/Sources/BridgeVMControl/HvfWindowsInstallPreparationStore.swift` | 109 | `e10913801f66b82f1f9ac6de48e049cc96b5bb2d5ff2991bf8473afd710abdc6` |
| `apps/macos/Sources/BridgeVMControl/LibraryControlModelFactory.swift` | 11 | `d70cd1eaea370b81c10c719c3f64a93eab3071759905bd233a75f36e421ef4a4` |
| `apps/macos/Sources/BridgeVMControl/LibraryDetailView.swift` | 33 | `67f06bc8767faf6d71e8c035a7c929fa2bb184059ada78bda91905e6b4f2daeb` |
| `apps/macos/Sources/BridgeVMControl/LibraryModel.swift` | 177 | `718807181febe7bbc996f3ab16ca20c04184dabd40e084c2a5f5c0eeef184c58` |
| `apps/macos/Sources/BridgeVMControl/LibraryModelSessionBindings.swift` | 40 | `10afaa7b0f456a86e5ee0fe1a7b603019750df285af3971b0b3b6ae0493b9a5f` |
| `apps/macos/Sources/BridgeVMControl/LibraryModelWindowsInstall.swift` | 46 | `b940619164275530477fcdae7277e112254883c5cf177f68dd247131ebc2c658` |
| `apps/macos/Sources/BridgeVMControl/LibraryModelWindowsInstallAccess.swift` | 33 | `96ced7ad4badbfaaa3bc31e274429780264b1a6d77bde4a5d54e5e046baaac07` |
| `apps/macos/Sources/BridgeVMControl/LibraryModelWindowsInstallPreparation.swift` | 45 | `d9d7733b03f38d8b14667922affe893833d497ece75f804a268fed3d8e467b13` |
| `apps/macos/Tests/BridgeVMControlTests/HvfRuntimeRetainedControlFixture.swift` | 116 | `cc027be777adc8fc1e9aa64c22e7b7dfc1f27304f43e8c068301de971f2cec8e` |
| `apps/macos/Tests/BridgeVMControlTests/HvfRuntimeSessionStoreFixture.swift` | 66 | `b748fa0e818c670d4deb821cd770655450a46c9bb67e72c1d857e167e33aa152` |
| `apps/macos/Tests/BridgeVMControlTests/HvfRuntimeSessionStoreTests.swift` | 264 | `d0d300310fe4d098b7f96c7f0a346afbd5153d4fd722d20c6383cf3425888f5c` |
| `apps/macos/Tests/BridgeVMControlTests/HvfWindowsInstallPlanPreparationFixture.swift` | 178 | `f24f98f1d1ff0f8ecac5b773fb9e8be44ce456b84ef1149767bab056c5109ebd` |
| `apps/macos/Tests/BridgeVMControlTests/HvfWindowsInstallPlanPreparationIdentityTests.swift` | 230 | `abd388ed4f322bb055b092b4510f494e76d687aa92b03b704ed2f16643dd2fa1` |
| `apps/macos/Tests/BridgeVMControlTests/HvfWindowsInstallPlanPreparationLifetimeTests.swift` | 133 | `d3bf16462fd9cafaeb93a05c3e0f34f387f3f874ebec293408d1444245adf040` |
| `apps/macos/Tests/BridgeVMControlTests/HvfWindowsInstallPlanPreparationProbe.swift` | 72 | `1dd14a502604078b72175a1a3c1ccc545fd77bab750e6ecbd0515f16682a121b` |
| `apps/macos/Tests/BridgeVMControlTests/HvfWindowsInstallPlanPreparationRoutingTests.swift` | 138 | `fb84665df7bf198b41c399d33e5cbd7057fbcb3aeb938d7fde3f1c147af36301` |
| `apps/macos/Tests/BridgeVMControlTests/HvfWindowsInstallSessionStoreTests.swift` | 200 | `28c2a9848b9b661fc9e7d694c7c95c6da680c2d06ce6aae1a2045e92d00891dc` |

The normalized extraction proof reconstructs all five original product files: default generic factory captures become explicit parameters with the MainActor closure contract; the install accessor's request fallback and Plan expression move unchanged behind a nonescaping cache-miss builder; the shared weak install binding replaces only its slug argument; the pending child type and explicit session injection account for the view differences. Private cache storage and existing policy stay intact. The reverse proof removes only the exact two-file asynchronous scheduling patch and restores the phase1 source hashes; the other ten product paths are exact, and all 197 other original source files—including complete Plan, Session, validation and identity files—remain unchanged.
Test preservation reconstructs the original install-view tests after five explicit shared-session injections and the runtime test methods after the declared fixture extraction/asynchronous setup. The retained-control fixture only strengthens its negative-route assertion. The new host intentionally reuses an identical ready handle without rebinding; the compatibility accessor preserves its old rebind behavior, and fresh prepared publication binds normally. No claim is made that host recreation erases hypothetical external callback overrides. Existing structural ceilings were respected.

## Elapsed-time follow-up

Separate source commit `010c4709845196f8644029563a1d40a03eaed2e3` contains the one-line elapsed-display change. The actual Swift build passed, exit code **0**, in **1.51 seconds**, against unchanged `b941ae18` plus that line. The resulting install view remains **126 lines**, SHA256 `9a3d20422fb424f4fba38d9b0e097714efd93baaa4676376664c1bb5fcfd7a42`; build-log SHA256 `98b7693d68f15a7df499b6f09bdaceb1003d457490a6d314259779a9bfc5f65d`. The follow-up replaces a preformatted elapsed integer with SwiftUI's standard [dynamic timer text](https://developer.apple.com/documentation/swiftui/text/datestyle/timer), retaining the existing active-only guard and styles. It changes the elapsed representation to a running clock and provides no measured rendered-refresh, real-media speed or installation-duration result. The 21-file table above remains the Plan-preparation source identity before that separate change.

## Preceding checkpoint and remaining validation

The preceding local project check passed in **159.68 seconds**, exit code **0**, with unchanged checked contents sealed as `b22abaa70d43ea0bc9c56cb509c26977fde54b8a`. At **2026-09-15 07:09:18 UTC**, that SHA had **78 hosted successes**, including [CI 34939361791](https://github.com/Ketchio-dev/bridgevm/actions/runs/34939361791) and [Security and quality 34939361784](https://github.com/Ketchio-dev/bridgevm/actions/runs/34939361784).
At this observation point, the current Plan-preparation/elapsed follow-up still requires the combined full project check and exact-SHA hosted verification. Deterministic tests establish execution venue, MainActor availability under a finite gate, UI-value routing, cache/attempt identity and lifetime. They establish no real-media latency, rendered SwiftUI dispatch or redraw, installation/guest completion, interrupted-read cancellation, filesystem watching or external-actor coordination. Cancelling an acknowledgment does not promise to interrupt a detached read. Existing Start admission and validation remain authoritative; A11/A19 stay open and no live result is promoted.

## Retained receipt identities

All paths below are relative to the operator's `hvf-continuation-12h-20260914` evidence collection and remain outside git.
Native log SHA256: first compiler failure `126a536f6359d91203c3bd6ed98aced3268bc5609cdfddfae29cb7da2fff63d4`; compiled primary baseline `6b36023c700567f5c880832aa6463b62351927ddf38c0747600905534c1f2894`; expanded pass `b3b18496d490f420ebfaa40d0555e3d758c4638366c8e08a3406213ebb917eb8`.
Native helper SHA256: `a51817734584797340fde03ef8afa6ad020272b3d73812a70505986c33518d1b`; preceding full-check log: `7caa070593f8e9c13e5eb403e7fd1ac4f162990d1d0d527bd7a208974cc67b39`.

| Receipt | SHA256 |
| --- | --- |
| `campaign/plan-preparation/plan-preparation-before-primary.json` | `a245a5b4e4edcf4c1926bd9e7612909c8e796c4468896c5323c00fbb28e88c88` |
| `campaign/plan-preparation/compile-failure-observation.json` | `730059165ecaf9c46eea6a6402dfe82f7d05b67a50033b44f7402c69edd07fc4` |
| `campaign/plan-preparation/plan-preparation-before-primary-compiled.json` | `675c259df468aca2a1e20bf461f7a725d8e47b68325c26d08262401cf1ea822e` |
| `campaign/plan-preparation/baseline-observation.json` | `3ee1cc50b6b7ce89cbb969fc5cee738bae1acd86509b7d37c1d7a66c6910c3d4` |
| `campaign/plan-preparation/prepared-tests-compiled.json` | `a90e8cae786bff8ebbeb0d7a904fe795c575f0e0e91d5d102aee592268e1425c` |
| `campaign/plan-preparation/plan-preparation-after-expanded.json` | `f665d1bdce28862791705ab168e3605a448acec2b9e50b89725b47424c796ca6` |
| `campaign/plan-preparation/phase1-source.json` | `60818f49419af0a25276f944ee02d1b4e26dc8ab1e2baabec29e6d8533b08d32` |
| `campaign/plan-preparation/phase1-preservation-proof.json` | `e7b9094d342bbd40cd0007eb067c8318ad5cff216dde888955da8fb11747ed74` |
| `campaign/plan-preparation/phase2-source.json` | `25f750f2426680262ee5ce78b0ee7a3a977cdf38bfba581916653cf1488fe4b8` |
| `campaign/plan-preparation/phase2-preservation-proof.json` | `002932d3dd90cb63a4cd0d92f591a424b6983b22921dbe49781804c2bcf4ad37` |
| `campaign/plan-preparation/test-preservation-proof-parent-final.json` | `5768e42bc141e0f4731cc9a8ec54b142ba09f86ded10d485e50fe02f4be0af2d` |
| `campaign/plan-preparation/product-after.json` | `4110f3d516c329893a7a37e2e43b78a07b860153d764fbec5a2ec512253725c4` |
| `campaign/plan-preparation/source-commit.json` | `af6c0f76fa2e3440e8ab6d2ca5ac6c2de1c52ba9078f3284df9dffb88b0d41b0` |
| `local-check-seal-retained-controls.json` | `a14e89511e66bbe15535e39f91aa5ebcdd6bf472f54987a21d31c3d0d3b90ce3` |
| `retained-controls-hosted-final.json` | `3d44f7b7bdf2f9054e2ff9362ffa697fa96e7278c9955398abcc83a6fe44bbb3` |
| `campaign/install-elapsed/elapsed-build.json` | `21e6572eda5005f1f78fe0f8a7e236c22677c624cd9a3d0d7239bbffcdabc844` |
| `campaign/install-elapsed/apple-timer-reference.md` | `694776fb33ea83423c39f0303aaf642541767aeae3d25580960663ca8c813ca4` |
| `campaign/plan-preparation/independent-final-review.json` | `b9b58ffdccf7864ba213ca600564b978914033fb33a760349a816bb74264174f` |
