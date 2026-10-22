# Retained lookup and accepted resource writes — 2026-10-21

This follows the [saved resource display and retained outcomes record](autonomous-resource-and-retained-outcomes-20261015.md)
and the [swift-testing toolchain record](hosted-swift-testing-toolchain-20261021.md).
It records automated tests and source review for the eleventh candidate. Product state,
capability wording, criterion states, thresholds and known defects remain unchanged.

## The tenth retained-terminal change broke an existing test

The tenth candidate `5014ad8a6a044585db775959be906a467043b210` was never published.
Its retained-terminal change `de5d9de749be7573337039c1a097f795c9490779` made lookup
replace any cached control that was retained and no longer active. The tenth record's
focused runs covered 11 methods, and its reviews found no blocker; neither ran the
complete suites.

On 2026-10-21 the complete BridgeVMControl shim suite, on that source plus the
accepted-write change below, recorded 1,442 passed, 1 failed and 3 skipped.
`HvfRuntimeSessionStoreTests.testBusyGenericModelDefersMetadataThenNextDetailUsesLatestWithoutReload`
failed two assertions: the selected control no longer kept its original configuration,
and the detail no longer held the same control. Natively the method failed at
`de5d9de7` with only the new resource tests added (10 run, 2 failures) and passed on
the main-equivalent `d1171eda` tree (10 of 10). The tenth focused results remain true
for what they ran; they did not include this method.

In that test a busy generic control is retained when its registration changes during
a reload, and becomes terminal when busy clears. The test pins it as the cached control
while the detail moves to the latest registration. The tenth defect concerned Start
after the user dismisses such a terminal record. Lookup now keeps a retained control
while it is active and, once terminal, until its retained record is dismissed; the next
lookup after dismissal builds the current registration's ordinary control. The old
control still cannot relaunch. Before dismissal, an ordinary Start on that registration
still resolves the retained control and refuses, as on main.

## Accepted resource writes keep their own control

An unrelated library reload during an accepted memory and CPU write could read the newly
written registration, treat the busy control as stale and retain it, after which ordinary
Stop refused. The accepted write now records the exact full configuration it expects to
persist. While that write is busy, a matching registration counts as the control's own.
Success and failure clear the expectation, and an already retained control never qualifies.
External replacement or removal after the write is still retained or pruned.

At `de5d9de7` with only the six new methods added, two failed with 11 assertions and four
passed. With the change all six pass. The methods use tiny owned registrations and fake
backends; no VM, helper process, guest or rendered window was exercised.

## Dependency updates

Two Dependabot commits are merged unchanged: serde_json 1.0.151 to 1.0.154
(`94f6c08af5a97d34e9121befa7aa0f1b4c2e848a`, `Cargo.lock` only) and the
dtolnay/rust-toolchain action pin `e2a55d2f` to `683b588c` across 19 workflow uses
(`5b275a928daa05d0fc705b1c2744fbf2b2c7bee3`). The new pin is the upstream default
branch head, one signed commit ahead, adding a release entry. Workflows still request
toolchain 1.85.0.

## Focused results on the eleventh source

| Check | Result |
| --- | --- |
| Native selection, 12 classes | 58 tests, 0 failures |
| Shim suites | 425 / 1,443 / 65 / 379 passed, 0 failed, 3 existing skips |
| First native attempt | helper script sourced without `ROOT`; zero tests ran; not counted |

Focused results do not replace the exact-source local full check or hosted checks; those
belong in the pull request record. No guest behavior or release criterion follows.
