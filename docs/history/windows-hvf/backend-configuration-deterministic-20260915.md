# HVF backend configuration: deterministic evidence, 2026-09-15

Historical evidence record. The [capability registry](../../../capabilities/windows-hvf.json) owns current status and wording; this record changes no criterion and does not close A11.
Source commit `8e5b975dbadddd25ffccb94f4f1051b14fe80ac9` separates the HVF backend from the shared backend file, forwards its explicitly supplied library root into launch configuration, and reports RAM/CPU from that same conversion.
This corrects implicit resource reporting from 4096 MiB/1 CPU to the existing launch defaults of 6144 MiB/4 CPUs. Explicit stored values remain unchanged.

## Retained failure and corrected result

The native baseline executed **5 tests**, with **8 failed assertions across 4 failed cases**, exit code **1**, in **7.58 seconds**. It exposed the ignored custom root, missed relocation journal, and implicit/mixed/pending resource-report mismatches. That failure remains recorded.
With the same test source, the expanded hosted product-flow filter plus `HvfWindowsBackendTests` and `LineAccumulator` passed **161 tests with zero failures**, exit code **0**, in **35.22 seconds**, starting **2026-09-15 03:50:09 UTC**.
The five new tests inspect configuration and reported resources, create/remove a tiny owned relocation journal through the actual readiness consumer, and cover nil/mixed values, explicit CPU16, and pending fallback. They trap process/key access and perform no Start, VM, guest-media, installer-pipeline, or WindowServer execution.
Configuration conversion may resolve helper metadata; the supported claim is read-only configuration inspection, not functional purity or zero filesystem reads. The broader filter also uses the existing native snapshot helper.

Both native receipts record base commit `15c52a4f80e6fa79fecff0127e10a5abcba10626` with the corresponding implementation in the working tree; sources and helper stayed unchanged during each run.
The following final file hashes match the green source receipt, native after-receipt, and committed contents at `8e5b975d`. Test SHA256 is identical in the before/after source and execution receipts. This establishes those files' identity, not a full check of the later commit.

| File | Lines | SHA256 |
| --- | ---: | --- |
| `apps/macos/Sources/BridgeVMControl/Backends.swift` | 493 | `0fe51728cedcef8fcc87216a4c62fb609c2daefce2daf474d3cbecaa1b3fa040` |
| `apps/macos/Sources/BridgeVMControl/HvfEngine/HvfWindowsBackend.swift` | 311 | `86c0acec3c263ba0b9a9f7ca942d6bdde7b460a06dfd40197f4eef19d45397f5` |
| `apps/macos/Tests/BridgeVMControlTests/HvfProductBackendConfigurationTests.swift` | 144 | `963c199f09cbe7b261ee241068ca163f4ac345b86d4cb82a02361b48f9513683` |

The original 308-line class has SHA256 `34878053a403796164fdf80ebdb495c68c9feb3b3bf41a833aa4a955ddf8f8ce`. Independent review verified exact extraction and restored those bytes by reversing only converter visibility, root forwarding, and resource reporting; all remaining backend bytes outside the removed section and its own separator are unchanged.
Independent source review found no introduced blocker. The generic factory's default root, resource editor's 10-CPU ceiling, and pending fallback without library context remain unchanged. These results do not establish all custom-library routes or live guest behavior.

## Preceding checkpoint and remaining validation

The preceding full local check passed in **163.77 seconds**, exit code **0**, at HEAD `947d16832d70d7d6984734a436fb00a40ec197bd`; its seal receipt records unchanged checked contents committed as `15c52a4f`.
At **2026-09-15 03:51:53 UTC**, the retained observation for that exact sealed SHA lists **77 hosted successes and 1 run still in progress**: [CI 34925954083](https://github.com/Ketchio-dev/bridgevm/actions/runs/34925954083). [Security and quality 34925953861](https://github.com/Ketchio-dev/bridgevm/actions/runs/34925953861) succeeded. This observation does not establish all 78 runs green.
At this observation point, `8e5b975d` still requires its full local and exact-SHA hosted checks. A11's final release regression seal remains required.

## Retained receipt identities

Names are relative to the operator's `hvf-continuation-12h-20260914` evidence collection; receipts/logs remain outside git. Backend receipts below share the prefix `campaign/backend-configuration/`.
Before log SHA256: `b84b03ba4950d6d1d5182977c579010438140a930288e4148856b1534d3cfa5e`; after log SHA256: `ae3a0c0d9ee00f1e85e6f248765456e437b82e7c3f7f42d7af03df5735a89745`.
Preceding full-check log SHA256: `e88f6f12d0bd8006e90ad1d0576e3ac299ce9da92e4cc20cc9f1ee7c3fd529c0`; native helper SHA256: `a51817734584797340fde03ef8afa6ad020272b3d73812a70505986c33518d1b`.

| Receipt | SHA256 |
| --- | --- |
| `red-source.json` | `1b33319fa4ae270d84ffe835b8cf306f83632d6ff9770960e2c08d4b8ebbd45d` |
| `green-source.json` | `f7b7e5b7d3d24a30f2781ca50e4d019ed10f160d3953b38006022048538f06cf` |
| `extraction.json` | `c42fc822ca457cec288695d3e4eeccc9340ffc25c4ec03aca65fba3cb8fed46c` |
| `backend-before.json` | `ea82fbe5d28cd57103e41fb153c4dc79b685be7131c7cd7020b69fc2b5f7cf10` |
| `backend-after.json` | `0338cd4d1f116a9eb8a001aa5b77a1c6f13865e2b69f8bb70e78342fa7b40f85` |
| `local-check-seal-runtime-continuity.json` (collection root) | `bcf4d745e29b0984094fa856c96d762fe44cb7c0b4181015d8b4a1e71d95431d` |
| `runtime-continuity-hosted-later.json` (collection root) | `29e42b8497e89cead76327daa971a0d9e50de42afeaf10a99b8a6476202ab6ce` |
