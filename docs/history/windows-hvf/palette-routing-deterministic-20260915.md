# Own-HVF command palette routing: deterministic evidence, 2026-09-15

Historical evidence record. The [capability registry](../../../capabilities/windows-hvf.json) owns current status and wording; this record changes no criterion and does not close A11.
Source commit `bbd65548309a5d5776b821ad9336b95c14d42dd2` directs own-HVF palette selections to their existing VM control screen. The command restores its captured selection, leaves Pro mode, and dismisses the palette. It does not directly start, stop, or refresh a VM.
Classification checks the experimental selection, current HVF registration, and retained active install/runtime sessions before generic model lookup. A cached generic model whose accepted configuration is HVF also receives navigation, including after it becomes idle without another reload. Compatibility-engine and global commands retain their existing behavior.

## Retained failure and corrected result

The native baseline executed **6 tests**, with **29 failed assertions across 5 failed cases**, exit code **1**, in **4.84 seconds**. The actual extracted factory returned generic commands or created extra generic models for own-engine selections; the experimental selection had no selected command. The compatibility case passed. The failure remains retained, and its generic command actions were never invoked.
With byte-identical test source, the expanded hosted product-flow filter plus `HvfWindowsBackendTests` and `LineAccumulator` passed **175 tests with zero failures**, exit code **0**, in **34.69 seconds**, starting **2026-09-15 05:00:51 UTC**.
The six new tests cover ready/pending/stopped HVF, all four active runtime states across pending/backend metadata changes, immediate validation and queued installation, experimental/empty selections, compatibility labels, and retained generic HVF through busy-to-idle. They invoke only an exact navigation command, asserting captured selection, Pro disabled, one dismissal, unchanged accepted sessions/configuration, and no additional owners or backend/process calls during that command's construction/action.
Fixtures use nonpolling generic models, injected runtime probes, and an injected validator. The accepted install acknowledgement is awaited before cleanup; queued installation work is discarded without execution. These tests invoke no generic command action, installer pipeline, VM launch, or window. The broader filter retains its existing native helper use; configuration setup may read helper metadata.

Both native receipts record base `e8d676f0ebc72fdd239530fa456163455e56319f` with the corresponding implementation in the working tree, unchanged source/helper hashes during execution. The following hashes match the final source receipt, passing native receipt, and committed contents at `bbd65548`. The test hash also matches the failed execution receipt and retained baseline source. This establishes file identity, not a full check of that later commit.

| File | Lines | SHA256 |
| --- | ---: | --- |
| `apps/macos/Sources/BridgeVMControl/CommandPalette.swift` | 56 | `75dae418435c349a82a34bb829fef3430b0f7572f262ed7f495104f6dab73a2b` |
| `apps/macos/Sources/BridgeVMControl/LibraryModelPaletteCommands.swift` | 33 | `2490f1b2f35b29d31c6e434ea04aaaf867b63d4cac6123eeb2a29672d10d590b` |
| `apps/macos/Tests/BridgeVMControlTests/HvfRuntimePaletteTests.swift` | 246 | `9b5e7000a2f438a2893429e0d5ced575a3a516beb78a9cb484e0cded8b9c6aec` |

Independent review verified exact extraction of the original selected-command block, reversing only its receiver adjustment. Removing the final added routing recovers those same generic command bytes; all other view/global-command bytes are unchanged. No introduced source blocker was found. Rendered navigation, live guest behavior, and Stop/Cancel reachability after a registration row disappears remain unproven: retained session objects alone do not establish that removed-row recovery path.

## Preceding checkpoint and remaining validation

The preceding full local check passed in **163.84 seconds**, exit code **0**, at HEAD `070c30298543e3c8101ee41b0ea52572a958defc`; its seal records unchanged checked contents committed as `e8d676f0`.
At **2026-09-15 05:01:44 UTC**, that exact sealed SHA had **77 hosted successes and 1 run still in progress**: [CI 34930626763](https://github.com/Ketchio-dev/bridgevm/actions/runs/34930626763). [Security and quality 34930626656](https://github.com/Ketchio-dev/bridgevm/actions/runs/34930626656) succeeded. This observation does not establish all 78 runs green.
At this observation point, `bbd65548` still requires its full local and exact-SHA hosted checks. A11's final release regression seal remains required.

## Retained receipt identities

Names are relative to the operator's `hvf-continuation-12h-20260914` evidence collection; receipts/logs remain outside git. Palette receipts below share `campaign/palette-routing/`.
Before log SHA256: `629b7720e28951709339cfb8fcc25e5326ce8bcb45d805c42891f50b4617cbdf`; after log: `402b61fdcaf94f27a461b5b7a2e363483d48c3800602496532c3dfe95c352856`.
Preceding full-check log SHA256: `33bfa3900c52b18a6bd388dfe38c9650a6745a39e30cf479cd0b4514fc4c1b89`; native helper SHA256: `a51817734584797340fde03ef8afa6ad020272b3d73812a70505986c33518d1b`.

| Receipt | SHA256 |
| --- | --- |
| `baseline-observation.json` | `0b79913a31b98145b4baf3eb75495c30f60f029e1a6fece721d2207bcb9adbdf` |
| `product-baseline.json` | `c9ee81ecd0bf2d276a411fa7eab673e2f1689a45098dcb97a332383e8389b584` |
| `product-after.json` | `a3056a3f0acf88fba2b3097d2faa01586979b4462625736750709eebd677a080` |
| `palette-before.json` | `65bef2d7e27f1b54edb002ca11b6e0c303316caa8d549034f97278f8ceba8399` |
| `palette-after-expanded.json` | `5750c20b542f8b8e635207441227fe28b50d57280a92e724044986a74838b9eb` |
| `local-check-seal-backend-root.json` (collection root) | `ad873d71f3000fde642271394013b600c5dd3ebf439963d88f8b0f689f4a4979` |
| `backend-root-hosted-later.json` (collection root) | `d546b273b7bafbccce8cb7f17da13a5283a5be4e777068b7e4495b917ad5e878` |
