# Native VM library design — 2026-09-15

The app adopts the supplied light, blue-accented VM-manager reference through reusable
SwiftUI components. The native sidebar remains beside the existing detail route. An adaptive,
searchable overview presents VM registration and saved CPU/memory configuration. New/create
and import actions lead to existing product flows; context actions keep their existing guards
and confirmations. Host capacity comes from actual host totals, without a utilization graph.

The registered VM name comes from the accepted HVF session configuration. Its observed state
remains in the runtime detail, where the unchanged display-open card now follows run/status.
No guest screenshot, running badge, account or unsupported navigation has been invented.
Native controls and semantic colors follow Apple's [macOS design guidance](https://developer.apple.com/design/human-interface-guidelines/designing-for-macos/)
and [color guidance](https://developer.apple.com/design/human-interface-guidelines/color).
These choices have not yet been verified in a rendered light/dark native window.

## Source and preservation

Source commit: `39c4437ed4afb3529f5cac098129d6deacde7dda`. Shared card, metadata, emblem and sidebar components separate
presentation responsibilities. Existing budget ceilings only decrease; new files are registered
at actual size. All 172 prior test files are unchanged. LibraryDetailView is byte-identical,
including first-run and retained-control route priority. Generic VM detail, root sheets and
confirmations, HVF configuration/recovery content, and the existing display-open action remain
byte-identical. No domain model, session ownership, guest media or security behavior changed.

Read-only review found two issues before sealing: a folder-derived runtime title and an import
instruction without an available action. The accepted VM name and actual overview Import
button resolve those findings. VoiceOver also receives the overview selection trait.

## Deterministic checks and boundaries

- Final native app build passed in **1.54 seconds**.
- **43 existing native tests** passed in **2.57 seconds** wall time, covering
  existing import navigation, runtime/install ownership, retained routes, plan preparation,
  command-palette routing and guarded library operations. The T17 entry test only checks its
  identifier constant; no actual create-button dispatch is inferred from that test.
- Whitespace and structural-budget checks passed with stable sources.
- No tests mirror the visual implementation. Search interaction, card activation, native button
  dispatch, keyboard focus, minimum-width layout and rendered light/dark appearance still need
  a native UI pass. The deterministic suites do not substitute for that pass or any guest gate.

The preceding checkpoint `4eb8a02bf7c9bab737d6cdb446bef9bab730469f` sealed a full project check
in **162.82 seconds** and all **78 hosted checks succeeded**: [CI 34949195210](https://github.com/Ketchio-dev/bridgevm/actions/runs/34949195210)
and [Security 34949195377](https://github.com/Ketchio-dev/bridgevm/actions/runs/34949195377).
Its pull-request checkout used a merge commit with identical source-tree contents; that
comparison is retained privately. Earlier failed experiments and hosted failures remain recorded.

At this frozen observation, this new app packet's full project check and pushed-SHA hosted
verification are pending. The [capability registry](../../../capabilities/windows-hvf.json)
keeps ENGINEERING_PREVIEW and every criterion unchanged; this design work supplies no live
Windows, performance, graphics, installation or release-readiness evidence.

## Retained receipts

Paths below are relative to the private `hvf-app-design-20260915` evidence directory.

| Receipt | SHA-256 |
| --- | --- |
| `source-preservation.json` | `0a9736d24d774b1a36e96af48ce1efc1401f48872b7b39e4875a557e7b25b75a` |
| `ui-build-final.json` | `db287b929eab0d695af3b8f1d187ed81484029b1c76f250cfd1e958ff86a5ca4` |
| `ui-build-final.log` | `232ac50cdcdda23faaa32aa64f15ae53ff3de6d21bb5049a7cb9231a2c9350cb` |
| `ui-native-final.json` | `8afc239e46ab0309e54b641687cd2579ef256068f56baa8c89e7eb1cb3542553` |
| `ui-native-final.log` | `b2030c4a6030aba6e03359c9976d604ece7b351fd26123a03adcf7862948e578` |
| `ui-focused-final.json` | `4bee87325531904c8b0e0ddb0e936a3aa32e060b2a4c71e7b7010ece82cead92` |
| `ui-independent-review-followup.json` | `2c38ba4077d46e51cae595171a72868f8dfabe2f293aa86dfb59211706a8cf79` |
