# Native app-UI diagnostic at main 97de386c — 2026-10-22

Classification: one live single diagnostic run plus static attribution. It is
not criterion evidence and changes no product wording. The operator approved
this physical run on 2026-10-22.

## Run

Job `bvm-97de386c-native-ui-v2-20261022-01`, tier `d6-app-ui-host-v2`, source
`97de386cc10d27fad3659ab65144467857ec7d5f` (the merge of #423, which contains
#417). Mac Studio (Mac16,9, macOS 26.7).

- **Inputs:** exact-source DEBUG host and driver; the release build boundary
  passed. Both bundles were signed with the existing Apple Development identity
  and passed `codesign --verify --deep --strict`. Manifest SHA-256 begins
  `227aef44`; host executable `e1c12666`; launcher `3801ddc5`.
- **Time:** started 04:08:41, finished 04:08:53 UTC.
- **Receipt:** `receipt.json` and the public copy are identical, SHA-256
  beginning `fb6352da`. `outcome=diagnostic-incomplete`,
  `failure_code=observation-refused`, `worker_cleanup_verified=true`.
- **Console:** unlocked in every 5-second sample. No Screen Sharing.

## What it observed

The driver processed six of seven requests with three mutations:

| Request | Outcome |
| --- | --- |
| welcomeControls | observed: create and import visible |
| pressCreate | performed |
| createForm | observed: commit visible, all three selections empty |
| cancelCreate | performed |
| pressImport | performed |
| importForm | observed: name visible |
| pressOverview | no reply |

Six screenshots were captured: four welcome presentations, the creation sheet
and the import form.

The previous run, `dev24-wave7-native-ui-v2-20261015-01` at `d3cace78`,
refused the first request with `AXChildren` `copyAttributeValues` -25201.
That refusal did not recur in this single run. #417 reads a relationship's
count before copying its values. One run cannot show that the refusal is gone.

## New failure: the overview phase creates control models

The host stopped itself with `Forbidden domain work: model_creations`
(tripwire count 1). It ends through `fatalError` at
`AppUIHostCapture.swift:42`, so macOS wrote a crash report and showed a
"quit unexpectedly" dialog on the console.

Static attribution (not proven by a test yet):

- Before pressing overview, the scenario seeds two fixture VMs
  (`seedOverviewMetadata`).
- The three-pane dashboard (`6709ffba`, 2026-09-16) shows
  `LibraryWorkspaceList` in the middle pane while the import form is open. The
  import-form screenshot shows that pane.
- Each `LibraryWorkspaceRow` takes `library.model(for: config)`. That call
  creates a `ControlModel` through the fixture's `modelFactory`, which is the
  tripwire. The row reads only `model.running`.
- The scenario and its tripwire predate the dashboard by one day (`77c9836d`,
  `8b3eb994`, 2026-09-15). No earlier live run reached this phase, so the
  conflict was never observed. The deterministic contract test checks only
  fixture construction, not the overview phase.

Either the list should render without creating control models, or the
diagnostic must stop counting that creation as domain work. That is a design
decision; neither change was made.

## Effect on the next physical job

The crash dialog stayed on screen. It was the frontmost window when T17 r82
was submitted. During T17 r83 it received a synthetic click at
04:45:08.400 UTC and opened Problem Reporter; see
[the A9 record](a9-r82-r84-and-t19-main-pilots-20261022.md). A diagnostic that
ends in a deliberate crash must have its dialog cleared before any later UI
job.
