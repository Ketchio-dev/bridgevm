# Runtime session continuity: deterministic evidence, 2026-09-15

Historical evidence record. The [capability registry](../../../capabilities/windows-hvf.json) owns current status and wording; this record changes no criterion and does not close A11.
Source commit `947d16832d70d7d6984734a436fb00a40ec197bd` retains own-engine runtime sessions at library lifetime, preserves active sessions before interpreting changed metadata, and replaces stopped sessions lazily.
Active installation keeps route priority. Runtime detail uses session object identity so a replacement receives a new view identity; appearance and edited-start guards require stopped state.

## Native result and source identity

The expanded native product-flow filter plus `HvfRuntime` and `LineAccumulator` passed **139 tests with zero failures**, exit code **0**, in **33.35 seconds**, starting **2026-09-15 03:29:51 UTC**.
The 14 new runtime tests comprise 10 store/detail tests and 4 admission tests. They cover actual detail-body construction, stable/replaced child identity, navigation/reload, library roots, stale busy-to-idle config, and all four non-stopped states with pending/backend changes.
The run recorded base commit `f2080e251bac4175b4fd18024279437e0f949d7a` with the later implementation in its working tree. Recorded sources and helper stayed unchanged during execution.
All nine packet file hashes below match the native receipt and committed contents at `947d1683`; their line counts match the retained budget record. This does not claim that the entire later commit was tested then.
Source filenames are under `apps/macos/Sources/BridgeVMControl/` (the two engine files under `HvfEngine/`); test filenames are under `apps/macos/Tests/BridgeVMControlTests/`.

| File | Lines | SHA256 |
| --- | ---: | --- |
| `LibraryModel.swift` | 187 | `66b0ab9c1082806a2aa4cf2882b97dd72ec1bab7f8e12c66e75546dda1ad142d` |
| `LibraryModelSelection.swift` | 15 | `6795688192a9fc976b371eddf9affd2d0bf5adafa6ec086e27d7fb098bd70e3a` |
| `LibraryModelHvfRuntime.swift` | 65 | `0bd0997b4e9b758e3222aec96d383e296ea2c08664766ce181dc531e3185a13c` |
| `LibraryModelWindowsInstall.swift` | 52 | `bb2b59a1014c4ac2f27e56463b3f5011ba6dc9e82286ade0394293738eafaeb9` |
| `LibraryDetailView.swift` | 40 | `577d6ec536b1b8c9f933dd3988232f9b47f6c2ceb6f62d335b48fa5f8ba60a09` |
| `HvfEngineView.swift` | 564 | `fc773408b36f15ab7e7249dea492988eced0f3db215ff17e42f07a87bba70421` |
| `HvfEngineSession.swift` | 547 | `ff4855cf21800ac73c6e5618b9c682f40ea3b07cff67a272caba2b17042587b1` |
| `HvfRuntimeSessionStoreTests.swift` | 285 | `b4841dcf8eb891e01656d1aa01f3071ed0729f5669d9f431ce4f97efb5d81570` |
| `HvfRuntimeViewAdmissionTests.swift` | 118 | `1202abad27209170aa454eba97dfd31bb0bb0d11fe569f01aed8096fb898f566` |

Native log SHA256: `7d1f4f98b87f86c7f7ab82c4ed74f729929dbb6a10b6f4fcb620084b24698c90`.
The existing native snapshot helper used by the broader filter has SHA256 `a51817734584797340fde03ef8afa6ad020272b3d73812a70505986c33518d1b`.

## Review limits and preceding checkpoint

Independent source review found no introduced blocker. Removing only the two new admission helpers restores the prior session source byte for byte, SHA256 `14ed106ff243a833f7e8970e251cf85d9be31f236f3339a06b124b27bd86ce00`; existing start/stop/deinit/poll and display-window bodies were not changed.
The original lifetime concern came from static ownership review. No guest termination was observed and no failing runtime experiment is claimed.
The new tests use injected false process lookup and state/object assertions. They never call runtime Start, execute a runtime process or installer pipeline, or use WindowServer. One install Start admission captures queued work that never executes.
Detail-body and display-surface value construction does not prove rendered state reset, window continuity, or live guest survival. Removed rows, external bundle changes, pre-existing simultaneous runtime/install work, child-only state rerouting, old display windows after stopped replacement, and process relaunch remain outside this evidence.

The preceding full local check passed in **150.03 seconds**, exit code **0**, at HEAD `16dc2bf927692d818da8a5907428bdef158626e1`; its seal receipt records unchanged checked contents committed as `f2080e25`.
Its log SHA256 is `e370b3ada434d9f636b72801336285d0665ca872d90c642c10871855640a5aec`. The seal preserves both preceding failed documentation checks; those runs remain failures.
At **2026-09-15 03:30:16 UTC**, all **78 hosted runs** for exact SHA `f2080e251bac4175b4fd18024279437e0f949d7a` were completed successfully, including [CI 34924509321](https://github.com/Ketchio-dev/bridgevm/actions/runs/34924509321) and [Security and quality 34924509072](https://github.com/Ketchio-dev/bridgevm/actions/runs/34924509072).
At this observation point, `947d1683` still requires its full local and exact-SHA hosted checks. A11's final release regression seal remains required; preceding results do not cover this source.

## Retained receipts

Names are relative to the operator's `hvf-continuation-12h-20260914` evidence collection. Receipts/logs remain outside git; this record contains their identities and measured outcomes.

| Receipt | SHA256 |
| --- | --- |
| `campaign/runtime-continuity/runtime-hosted-filter.json` | `23460f45eca830903ff1cfbe66ca894da8fa628ffb8f398436ecfb503e4e235a` |
| `runtime-continuity-budget-counts.json` | `d17374ca629bf79049f19ec60b32cb4099405ec5217222e76373856af41fd59c` |
| `campaign/runtime-admission/source-receipt.json` | `fb5d2381c9b2064848ef7df1be066a3e5b6434a2776742d5fcba5cbd92fc28a4` |
| `local-check-seal-install-continuity.json` | `9ad6deaee6ee2396cab057ec4b2328f5b26c02a1a0b181fda8b0da9d8d66dfd3` |
| `install-continuity-hosted-later.json` | `b5b2fd0294796cf43f06a257c95b32e62e224b63d0d6ee6abc01fe96cce6ebd2` |
