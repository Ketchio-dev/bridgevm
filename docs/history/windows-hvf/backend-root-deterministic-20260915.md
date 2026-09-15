# Library-owned backend roots: deterministic evidence, 2026-09-15

Historical evidence record. The [capability registry](../../../capabilities/windows-hvf.json) owns current status and wording; this record changes no criterion and does not close A11.
Source commit `070c30298543e3c8101ee41b0ea52572a958defc` forwards an explicit library root through the HVF backend factory and the default `LibraryModel` control-model factory. The three uncached deletion/clone/move backend fallbacks also receive their owning library's root.
The default factory still starts model polling unless explicitly disabled. A supplied model factory remains authoritative. Standalone `ControlModel(config:)` and omitted-root backend construction retain their existing global-root defaults; Fast VZ and QEMU behavior is unchanged.

## Retained failure and corrected result

The native baseline executed **3 tests with 2 failed assertions**, exit code **1**, in **5.21 seconds**. Both the actual configuration factory and actual default library factory supplied the global root instead of the owned test root. Each failing path checked the launch context and returned before the sole backend resource write, preventing writes to the global library. This failure remains retained.
With byte-identical test source, the expanded hosted product-flow filter plus `HvfWindowsBackendTests` and `LineAccumulator` passed **169 tests with zero failures**, exit code **0**, in **34.70 seconds**, starting **2026-09-15 04:39:43 UTC**.
The three new tests create tiny same-slug registrations in two owned roots, verify resource changes update only root A while root B stays byte-identical, and verify neither VM bundle is created. They exercise the actual default library factory with polling disabled, stopped-model replacement after reload, and explicit-factory precedence. They perform no Start, process, helper execution, key access, guest-media, deletion, clone, move, or WindowServer work. Configuration conversion may read helper metadata; this is not a zero-I/O claim. The broader filter retains its existing native helper use.

Both native receipts record base `5c3bcad916660a813813b67af9d2b3976cbbaa0f` with the corresponding implementation in the working tree, unchanged sources/helper during each run. These four final hashes match the source receipt, passing native receipt, and committed contents at `070c3029`; the test hash also matches the baseline source and failed execution receipt. This establishes file identity, not a full check of that later commit.

| File | Lines | SHA256 |
| --- | ---: | --- |
| `apps/macos/Sources/BridgeVMControl/Backends.swift` | 493 | `7705271e12b0428167d91de8d3116145974e4009499c8a6ed56a2df75f743f71` |
| `apps/macos/Sources/BridgeVMControl/LibraryModel.swift` | 179 | `fbecc5858bcdbc4704786dcb479293dd21cdb300b65c2b7a144f12a6eeb3ca9c` |
| `apps/macos/Sources/BridgeVMControl/LibraryModelActionRequests.swift` | 13 | `6d9d6409fffd5626183ad66aede21a29258fda8ba260b46384c1a5c3b98b3343` |
| `apps/macos/Tests/BridgeVMControlTests/HvfProductBackendConfigurationRootTests.swift` | 119 | `bc94cb3fd2ac7c8baa0fd3dfe8534b9f3d4368782408e37c86e2dddf5329e21d` |

Independent review verified unchanged `ControlModel` source, exact extraction of both request methods, and unchanged operation bodies apart from their root arguments. Reversing only the factory parameter and HVF root forwarding restores all previous shared-backend bytes. The three action fallbacks have static wiring evidence only; no deletion, clone, or move was executed to prove their behavior. App-owned action admission, external concurrency, and live guest behavior remain outside this result.

## Preceding checkpoint and remaining validation

The preceding full local check passed in **162.93 seconds**, exit code **0**, at HEAD `ce73620c8826d829e642750c91a90799b8cfe658`; its seal records unchanged checked contents committed as `5c3bcad9`.
At **2026-09-15 04:41:20 UTC**, that exact sealed SHA had **77 hosted successes and 1 run still in progress**: [CI 34929273250](https://github.com/Ketchio-dev/bridgevm/actions/runs/34929273250). [Security and quality 34929273352](https://github.com/Ketchio-dev/bridgevm/actions/runs/34929273352) succeeded. This observation does not establish all 78 runs green.
At this observation point, `070c3029` still requires its full local and exact-SHA hosted checks. A11's final release regression seal remains required.

## Retained receipt identities

Names are relative to the operator's `hvf-continuation-12h-20260914` evidence collection; receipts/logs remain outside git. Root-factory receipts below share `campaign/backend-root/`.
Before log SHA256: `f70145241df0122c58f4658bd9b0ca45d80f7e4abd03e1f7525a551ddc1b6618`; after log: `10912f3cd2a89e86f911930ee4cb533f4f1f26bd546443f2b24be2957b5e06cb`.
Preceding full-check log SHA256: `2c9e1f4a3a45680969b30030990f45ed7807e262b72aa665a674d836e91b4733`; native helper SHA256: `a51817734584797340fde03ef8afa6ad020272b3d73812a70505986c33518d1b`.

| Receipt | SHA256 |
| --- | --- |
| `baseline-observation.json` | `a181c60be53790d494322d3c4a2c7a80a71d783685c898a6e5c352f2cd5234f3` |
| `action-request-extraction.json` | `119ea92bed0e56d74ec19e99cad55cd3aee3cccf0e2535b3afbd20b520e4b5d6` |
| `product-after.json` | `f2499fff350969f9c0f20d8f19e70a99e6e0e0572b0f6c62768eb26732831f02` |
| `root-before.json` | `bfb7220985ba240e593cbdd22e050e9bd589c30b4014f85f5c38b9a7a88318e4` |
| `root-after-expanded.json` | `b3c80a5fec91ab3b09b72fe09a92ba6767a0486f35757a1ccc509cad98c91636` |
| `local-check-seal-install-validation.json` (collection root) | `9b7bb3c57619cd69c63c4c39abe223e4983d2fcd84e3e0c41aaad824429950f7` |
| `install-validation-hosted-later.json` (collection root) | `a2f3088f504ffcf92f3263cc33e0b60fd2173edaf5e8a24387af04daf1316787` |
