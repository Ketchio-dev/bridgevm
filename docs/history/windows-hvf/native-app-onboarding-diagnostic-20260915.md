# Native app onboarding and diagnostic preparation — 2026-09-15

Source `1fb301f0c191cc26e2ad6c3c9d0ed0979fef3f5a` includes onboarding `63a1592049be3201350ecba8b431cc7c41e3564c`
and navigation correction `186c41f6f57780a716f477bffd6f3f29236e6b89`.
The first-run screen offers real create/import choices with shared native styling.
Import inputs group VM name, files and saved CPU/memory configuration. Existing
chooser behavior, defaults, media validation, import/recovery algorithms and
runtime ownership remain unchanged. All 172 preceding native test files are
byte-identical. New files are budgeted at their actual sizes; no ceiling rises.

A review found that a published import reached through Overview then Pro-off could
return to welcome after Other Import. The accepted return now selects editable
import inputs. Busy or unpublished transitions refuse before changing state.
The unchanged old transition was run against three new regressions: the affected
test had three assertion failures and the two refusal tests passed. The corrected
run selected 56 tests: **55 passed, one intentionally skipped, zero failures** in
24.83 seconds. The skip is the opt-in native window diagnostic. The app build
passed in 0.56 seconds. A preceding harness compile failure (namespace versus
protocol) is retained separately and is not behavioral failure evidence.

## Native diagnostic boundary

The new d6-app-ui queue tier executes a precompiled, hashed XCTest executable at
its exact sealed commit through the existing GUI physical-Mac worker. It does not
build on the live queue or boot a VM. An owned NSWindow hosts the real ContentView;
empty owned library fixtures disable migration and automatic models. Tripwires
stop before runtime/model/install construction or file jobs. Two overview rows
are synthetic metadata, never activated. The create sheet is canceled; media
selection values must be empty before capture.

Success requires eight owned-view PNGs, seven actual accessibility observations,
zero domain-work tripwires and matching image/report hashes. The runner has a
90-second deadline and bounded cleanup inside the worker's process group. PNG
checking now rejects invalid compressed scanlines, including the independently
retained CRC-correct malformed-image counterexample. Policy checks pass **11/11**;
prior synthetic file-permission fixture failure is retained. Hosted macOS selects
the new onboarding tests and the mandatory opt-in skip; Ubuntu runs queue policies.

At this frozen pre-pilot observation, **no native UI run or screenshot inspection
has occurred**. A default test skip, successful build or policy test proves no
rendering, focus, search interaction or guest behavior. Full project and exact
pushed-SHA checks for this packet are pending. Subsequent pilots must retain failed
attempts and stay app-only diagnostic evidence, with claim eligibility, criterion
pass and capability promotion all false and guest boots zero.

## Previous checkpoint and retained failures

Checkpoint `05ebab4fa5aaa51a9ee1930bde29545d4d69748d` sealed its full project check
in 161.17 seconds. At 2026-09-15 13:21 UTC all 78 hosted workflows succeeded:
[CI 34970421159](https://github.com/Ketchio-dev/bridgevm/actions/runs/34970421159)
and [Security 34970421424](https://github.com/Ketchio-dev/bridgevm/actions/runs/34970421424).
[Run 34970415627](https://github.com/Ketchio-dev/bridgevm/actions/runs/34970415627)
attempt 1 failed at an unchanged Windows fixture's 10-second child wait. Its cause
is undetermined; stdout/stderr were not captured by that timeout path. One unchanged
failed-job retry (attempt 2) passed. Both complete logs remain; the first failure
is not rewritten as success and no deadline/assertion was weakened.

ENGINEERING_PREVIEW and every capability criterion/threshold remain unchanged.
A11 still requires its release regression seal. No installation, Windows runtime,
graphics, performance or release claim follows from this app work.

## Private receipt hashes

These receipts stay outside git; paths below identify retained records, not guest assets.

| Receipt | SHA-256 |
| --- | --- |
| hvf-app-ui-diagnostic-20260915/onboarding-preservation.json | `22f4af9645ded7aab85e604d7407bcc5ef60c034e0379eaf2c865357535a9701` |
| hvf-app-onboarding-20260915/navigation-before.json | `e0859669ca64456e9afd476338f6b3fece52d186f71d20c28ca1e857ec9aad89` |
| hvf-app-onboarding-20260915/navigation-before-02.json | `94da5691872992eb671bcd69005f4ed8978c4980b23faaf6cdec1b005edbab2f` |
| hvf-app-onboarding-20260915/navigation-corrected-final.json | `768fe9801edc28eddac45523a93d857d0d1a08bc8f60260cdda5367d46e4f811` |
| hvf-app-ui-diagnostic-20260915/ui-app-build.json | `ce9bddcc37c5ff9c09f63bb21f94c368ed29cb1e2b99eaa59d3e06af035c2849` |
| hvf-app-ui-diagnostic-20260915/ui-policy-final.json | `b72dacb3e1ca40d239a7cb8a71e096d60be405211376b1282364e8e1d90a1bec` |
| hvf-app-ui-diagnostic-20260915/ui-budgets-final.json | `9a45216ca057426c8c249413ed90001f353ef4b60780f6761d7e3b4a965d1110` |
| hvf-app-design-20260915/hosted-resume-01.json | `abdbb23f3e3a204561c477a1d2687e3f2f79c9c7adf2102bee36606f57f94216` |
| hvf-app-ui-diagnostic-20260915/independent-ui-diagnostic-review.md | `7d3c12ea5b3be248d7a20ce7ceefa0f584dab736ed2974c99b011ab55b60a8cd` |
