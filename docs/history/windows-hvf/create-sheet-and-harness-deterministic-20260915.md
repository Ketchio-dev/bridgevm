# Create-sheet extraction and typed input-harness repair — 2026-09-15

This record separates a structural source move from a standalone diagnostic-harness repair.
Both preserve existing product behavior and test criteria; neither establishes a guest capability.
The [capability registry](../../../capabilities/windows-hvf.json) owns current wording and status.

## Preceding local result and retained hosted failures

G/E2/H checkpoint `92f8be3c834f67e2f35cbcec8b93c4172d5bb02a` sealed a passing
`scripts/check-project.sh` result in **161.03 seconds**. That local pass did not establish hosted success.
The exact same SHA then failed both hosted Production Swift input driver transport contracts runs:

- [Push run 34947027845](https://github.com/Ketchio-dev/bridgevm/actions/runs/34947027845), job `104308770470`.
- [Pull-request run 34947030512](https://github.com/Ketchio-dev/bridgevm/actions/runs/34947030512), job `104308777043`.

Both failed compilation at `scripts/live-gates/production-input-driver.swift:89`, where unary `!`
was still applied to `HvfNegotiatedInputStream.Admission` after the product API stopped returning Bool.
The integration steps did not run. The source list already included the required type; this was a stale
caller, not a missing dependency. These failures remain failures, and earlier records are preserved.

## I — complete creation-screen extraction

Source commit: `9222b1593129a49ecca4a6f94600f56249aaa371`. The complete existing CreateVMSheet moved beside
its creation/result extensions. No access levels, nested types, imports or creation algorithms changed.
CreateVM retains original lines 1–497; the new 336-line file holds the unchanged original 499–828
sheet span after six copied import/separator lines. Adding the explicitly retained original line-498 LF
reconstructs all original 828-line bytes: `4c65289b293a442b81260a0681969b7c8e68bbb29fa14f7b1659fd084817c214`.

The first focused attempt passed the existing injection-deny and budget checks, then failed
`git diff --check` on the new blank line at CreateVM.swift:498. **No Swift build ran in that attempt.**
Only that one separator byte was removed. The retained follow-up smoke, budget and whitespace checks
passed; the actual Swift build then passed in **2.39 seconds**, with all 384 Swift source identities stable.

The scanner changes only three source-path memberships, retaining both creation logic and moved UI.
All existing patterns, thresholds, messages and positive test checks remain unchanged. The only budget
changes are CreateVM 828→497 and new CreateVMSheet 336. All **382 other prior Swift files**, including
**172 prior test files**, remain byte-identical. No additional test mirrors this complete-type move.

| I source path | Lines | SHA-256 |
| --- | ---: | --- |
| `apps/macos/Sources/BridgeVMControl/CreateVM.swift` | 497 | `793b07115eb4df2aa0dee4164e82eb5ec83c3ee2d8cbc5f19e54fb17ff05366f` |
| `apps/macos/Sources/BridgeVMControl/CreateVMSheet.swift` | 336 | `ea4a07601456a32c7aa54ca6d6ae01e460258094ec4e673354c6596a35d28afc` |
| `tests/integration/hvf-windows-product-injection-deny-smoke.sh` | 17 | `95e4ffbd7d634389dfc01eb623699e6eeb0fae485f45de713b5a4b6551d95227` |
| `scripts/refactor-budgets.tsv` | 2393 | `1a2a638d2257201c7053127537c0a2f5a1c0f78d4d1dabd64abb50c81aa88866` |

## J — standalone typed-admission caller repair

Source commit: `748d6d676ac97b28e1f7bd2d131c4f03d39d2bb2`. Only the harness condition changes from
`!driver.route(...)` to `driver.route(...) == .legacy`. This preserves the old legacy-fallback branch;
refusal still follows the existing diagnostic callback, and queued input keeps the production pump.
The product driver, build source list, workflow and existing Python tests are unchanged.
The harness remains **139 lines**, SHA-256 `3e912763f0482d610a05efcf4b8a8ede39783b547598ad8a22317eb4b7a50137`.

The actual standalone build passed in **4.63 seconds**. The unchanged Python suite passed its **two
methods** in **1.87 seconds** wall time (the unittest output reports 1.826 seconds); the recorded combined
run took **6.51 seconds**. It checks an ordered burst and rejection on bad counts/restart with an owned
fake serial peer. The before/after validation identities are stable; no VM or rendered UI was used.
Local binary SHA-256: `878245a35e82b3097f6aacb3a294c9c7cc93e91bb07a1fee09469a8164520927`.

## Receipt identities and limits

Receipt paths are relative to the retained campaign evidence root. Initial 498-line extraction receipts
also remain present; the EOF-suffixed receipts identify the corrected state and do not replace the failure.

| Receipt | SHA-256 |
| --- | --- |
| `local-check-seal-keyboard-draft.json` | `7354a6e3eed1b18e56cb6e401328a3386eb990b7ff0598f57cbcfdadfa4504b5` |
| `campaign/create-sheet/prebuild-failure-observation.json` | `081de8628a6fa23a7d62517ce569b561c097e396397334573e9e85d6175266c9` |
| `campaign/create-sheet/focused-check.log` | `10c4f6e83cfdb58a49145241783939a6704a58e868e59613bf6513bbad947d97` |
| `campaign/create-sheet/focused-check-after-eof.log` | `95ac0b9371e3515d19ffbe3e90c8a90e5a9c3065f6ca4ff246d68d8a331e80a7` |
| `campaign/create-sheet/build.json` | `7120ad0793f9c8a597613755196b6487a3dbef46128cfc8ee91a94f8cf3cde50` |
| `campaign/create-sheet/build.log` | `ddcd4bf6cdb6821522b2a4072fac2d71ea6892bffe445fa0300f8741276c4c2a` |
| `campaign/create-sheet/source-after-eof.json` | `97cc3a4bd8302f89739be11c1e1a1291b7771fb2669711e4f9a1e794fdd1596d` |
| `campaign/create-sheet/preservation-proof-eof.json` | `4f301c82d955fd12bf0b14b659aef0ebbc394dcb498f975e35b9590cec9cf5db` |
| `campaign/create-sheet/independent-final-review.json` | `67309d1c8d04ec37722877f07e68bf17415fda31be31db7f6e097c74e208251b` |
| `campaign/create-sheet/source-commit.json` | `88e397ade728abb562dab430d9b9b86bd7a8abf81370f88f18196ac69d399c44` |
| `campaign/keyboard-hosted-contract/diagnosis.json` | `70b6708591c03fd80c7a63c5a73d4289633ab2db2df1eb849ea596721a70f0f1` |
| `campaign/keyboard-hosted-contract/receipt-34947027845.json` | `9ce75c4b622e029a19786adb4fe216a875d5f1790e6b39bc646f0a0184114fa0` |
| `campaign/keyboard-hosted-contract/job-104308770470.log` | `2fc96d7d2a958b0be7b449586c5f22fbd0f51c21763b7e724112e6e6eb8d5ad2` |
| `campaign/keyboard-hosted-contract/receipt-34947030512.json` | `bb5b12ec213b3e1b4c7ff81fbb4bd274f907f4550b8821b3502652d47669c06e` |
| `campaign/keyboard-hosted-contract/job-104308777043.log` | `38c8ef3e2cec19dbb93e8e72d1abd2d8047533751c15719c266fea818baa11bd` |
| `campaign/keyboard-hosted-contract/after-local/validation.json` | `d5df266e582c4dac668e4ce435662770103d013f85f00cab1a18629ff171ace8` |
| `campaign/keyboard-hosted-contract/after-local/integration.log` | `983e627acfaefb0df3ba4c5d5a26f793c2aae58564f032b9466191051afbf064` |
| `campaign/keyboard-hosted-contract/source-commit.json` | `b7c39adfb93d9d4a7067083d34d41f48a17d50f63bd25eeb9226adfd8787a241` |

At this observation, the combined I/J full project check and exact pushed-SHA hosted verification remain
pending. Compilation, source reconstruction and a synthetic serial contract do not establish rendered
button dispatch, Windows insertion, application consumption, installation behavior or performance.
A11/A19 and product-state criteria are unchanged. No threshold was relaxed and no failed record was rewritten.
