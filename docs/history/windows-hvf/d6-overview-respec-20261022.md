# d6 native-UI overview re-spec — 2026-10-22

Classification: deterministic tests plus one local reproduction on a
development Mac. No live d6 run has exercised this change. It is not criterion
evidence and changes no product wording.

Branch `agent/d6-overview-20261022` from `d1f17b2d`: `a26929b2` (inert-backend
fixture models and the exact verifier) and `72d8f454` (overview re-spec). It
covers `d6-app-ui-host-v2` (driver), `d6-app-ui-host-v1` (legacy) and
`d6-app-ui` (XCTest).

## Live failure

The [native app-UI diagnostic at 97de386c](app-ui-417-live-diagnostic-20261022.md),
job `bvm-97de386c-native-ui-v2-20261022-01` (tier `d6-app-ui-host-v2`, receipt
beginning `fb6352da`), ended `diagnostic-incomplete` with
`observation-refused`. The driver processed six of seven requests. At the
overview phase the host stopped itself through `fatalError` with
`Forbidden domain work: model_creations` (count 1).

## Cause

This was reproduced at `d1f17b2d`. The reproduction used the host's own fixture
factory, the real first-run import action and the real sidebar overview button
action, reaching both actions through their SwiftUI values. Building the first
row's model stopped the process at `AppUIHostCapture.swift:42` (exit 133). The
saved report showed `model_creations` 1, and the completion record showed
`success` false. Two stale assumptions date from the three-pane dashboard
(`6709ffba`), which landed one day after the scenario:

1. Every `LibraryWorkspaceRow` takes `library.model(for: config)`, and the
   fixture's model factory was itself the `model_creations` tripwire. The row
   reads only `model.running`.
2. Even without that tripwire, the overview wait could never be met.
   - **Old button:** it set Pro mode with no selection.
   - **Current button:** it leaves Pro mode, and selects the first VM only
     when nothing is selected. Pressed from the import form, it leaves the
     selection on the first-run import pseudo-selection
     `__bridgevm_first_run_import__`.
   - **Old wait:** it expected Pro mode with no selection, and stayed false.
     The view it expected, a fleet table in the detail pane, would be a second
     workspace list.
   - **Old row matcher:** it looked for the old card label `<name>, HVF,`.
     Rows are now labelled `<name>, <state>, <cpu>  ·  <memory>  ·  <engine>`.

## Decision: option B

The operator's delegate took this decision under the operator's delegation.
**It changes a diagnostic check after that check failed live.** The
[original boundary](native-app-onboarding-diagnostic-20260915.md) stopped
"before runtime/model/install construction or file jobs" and required zero
tripwires. Under option B, a control model no longer counts as domain work.
The invariant is now stated as "no backend, process, Keychain or file effect".
A model by itself has none of these.

This is not a relaxation of that safety invariant, for these reasons:

- **Inert backend.** Each seeded row's model is a `ControlModel` over
  `AppUIHostInertBackend` (`HvfAppUIInertBackend` in the XCTest tier). Every
  effectful backend method is a counted, fatal `backend_calls` tripwire:
  `isRunning`, `currentIP`, `start`, `stop`, `resources`, `setResources` and
  `runInGuest`. The capability flags are constant `false`.
- **No polling.** Models are built with `startsAutomatically: false`.
  `ControlModel.init` starts refresh and polling only when that flag is true,
  so no timer can reach the backend.
- **Exact count.** The count is exact, not a range:
  - Before seeding, any model creation is fatal.
  - After seeding, exactly one model per seeded fixture VM is allowed, which
    is 2.
  - A duplicate, a model for an unseeded configuration or a third request
    stays fatal.
- **Other tripwires unchanged.** `runtime_creations`, `install_creations` and
  `file_jobs` are still fatal.
- **Verifier.** The shared verifier is
  `scripts/live-gates/app_ui_diagnostic.py` with
  `scripts/live-gates/app_ui_domain_work.py`, used by all three tiers. It
  requires, as exact integers, `model_creations`
  `{before_seeding: 0, after_seeding: 2, seeded_fixture_vms: 2}` and all four
  tripwires at 0. It refuses:
  - 0, 1, 3 or 4 after seeding
  - any creation before seeding
  - any backend call
  - a redefined seeded count
  - the old zero-model report

Option A was not taken. It would have rendered the list without control models,
which means changing product views.

## Overview re-spec

- **Wait.** The scenario waits for the state the button produces: Pro mode off
  and the first-run import selection kept. No VM is selected. Once rows are
  seeded, the button never leaves the selection nil: from nil it selects the
  first VM.
  - The host and driver use `AppUIHostOverviewState`.
  - The XCTest tier is compiled without the host flag, so it holds the same
    predicate in `HvfAppUIOverview`.
- **Rows.** A row is matched by its exact label on a button:
  `Amber, 정지됨, 4 vCPU  ·  4 GiB RAM  ·  HVF` and
  `Indigo, 정지됨, 4 vCPU  ·  8 GiB RAM  ·  HVF`.
  - The host and driver share one definition, `AppUIDriverOverviewRows`.
  - A second match is refused as ambiguous.
  - A stale label, another state or a non-button does not count.
  - Filtering and clearing use the same matcher.

Counted at `d1f17b2d` and at `72d8f454`:

| Scenario | Actions | Captures (named + welcome) | Waits | Mutations | Overview wait predicates | Driver requests |
| --- | --- | --- | --- | --- | --- | --- |
| d6-app-ui-host-v2 (driver) | 7 → 7 | 8 (4 + 4) → 8 | 8 → 8 | 6 → 6 | 4 → 4 | 12 → 12 |
| d6-app-ui-host-v1 (legacy) | 7 → 7 | 8 (4 + 4) → 8 | 7 → 7 | 6 → 6 | 4 → 4 | — |
| d6-app-ui (XCTest) | 7 → 7 | 8 (4 + 4) → 8 | 8 → 8 | 6 → 6 | 4 → 4 | — |

Before and after alike:

- **Host capture contract:** 8 screenshot names and 7 action names.
- **Driver policy:** 7 phases and 6 mutating operations.

No phase, capture or assertion was removed.

## Tests

Each test below failed before its change and passes after it.

| Test | Before | After |
| --- | --- | --- |
| `tests/integration/app-ui-host-model-creations-contract.py` (8 tests) | against the `d1f17b2d` verifier: 1 failure, 3 errors | OK |
| `tests/integration/app-ui-host-tier-smoke.py` (19) and `tests/integration/app-ui-tier-smoke.py` (11), given the exact report | 3 and 4 errors | OK |
| `AppUIHostOverviewContractTests` (fixture counting) | does not compile at `d1f17b2d`; see the reproduction above | passes |
| `tests/integration/app-ui-driver-ax-overview-contracts.swift` | old matcher stops at "both current rows observed once beside their menus" | 50 AX checks pass (39 before) |
| `AppUIHostOverviewStateContractTests` (real import and overview button actions) | does not compile at `a26929b2`; with the old spec mapped onto the new names, 6 assertions fail | passes |
| `HvfAppUIOverviewContractTests` | does not compile at `a26929b2`; with the old spec, 5 assertions fail | passes |

Other checks that were run:

- `scripts/check-app-ui-driver-contracts.sh`: PASS.
- Apple XCTest host contracts from `scripts/check-app-ui-host-contracts.sh`:
  11 of 11 pass, with the exact inventory. They were run from an evidence copy
  of the script that only adds `-j 4`.
- `scripts/check-python-scripts.py`, `scripts/check-refactor-budgets.sh` and
  `scripts/check-documentation-system.sh`.
- The production driver build, `apps/macos/scripts/build-app-ui-driver.sh`. It
  was compiled only, not run.

A local probe hosted the real `LibraryOverviewView` in a windowless
`NSHostingView`. It built exactly two inert models with zero tripwires, but
without a window it exposed no row accessibility. The tests therefore match
rows against owned elements, not against the rendered list.

Not run:

- `scripts/check-project.sh`, which the operator excluded.
- Hosted CI, because nothing was pushed.

The logs stay outside the repository, in the operator's evidence directory
`dev-20261021-evidence/d6-overview`.

## Not validated

No live run of `d6-app-ui-host-v2`, `d6-app-ui-host-v1` or `d6-app-ui` has
exercised either commit. Until one does, these remain unknown:

- whether both rows are on screen with these labels in a real window (the list
  is lazy)
- whether the overview, filtering and clearing phases complete
- whether any other view requests a model or calls the backend

A tripwire still ends through `fatalError`. Its crash dialog must be cleared
before any later UI job.
