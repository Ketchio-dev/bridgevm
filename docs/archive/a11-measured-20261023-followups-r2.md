# A11 observation replaced after the failed follow-ups seal on 2026-10-23

Classification: historical evidence only. The [registry](../../capabilities/windows-hvf.json)
and [capability matrix](../windows-arm/capability-matrix.md) own current state.
This file preserves, verbatim, an A11 note that a later checkpoint replaced. Copying it
does not revalidate it, close A11 or establish a release result. Earlier observations
are in the [follow-ups archive](a11-measured-20261023-followups.md) and the archives it lists.

## Follow-ups seal 1c569fb9 (failed its local full)

Seal `1c569fb9` recorded this note for source `fefd91ff`. Its local full failed:
`clippy (workspace)` and `clippy (venus)` rejected an unused `Result` in a new HDA
diagnostics test. The test came from the audio branch and was written before #426 made
`VirtPlatform::reset` return a `Result`. Every other stage passed. Commit `de347fb2`
fixed the test, and the next seal replaced this note. It is preserved here.

- Source commit: `1c569fb944fa04c07da511f24d7f3f35bf7e3c8c`.
- Source field: `capabilities/windows-hvf.json`, criterion `A11`, `measured`.
- Source registry SHA-256: `4654918840146378bb0e8a0046cbad5da1be941f15aaf875922a108b17b9e547`.
- Decoded UTF-8 measurement SHA-256: `09754ebb4c4a133a0c02473649383d16c606f76c8002d29a52160fdb38da9a04`, 2204 bytes.

<!-- BEGIN ARCHIVED A11 MEASURED -->
2026-10-23 03:02 UTC checkpoint: A11 remains OPEN. Main 97de386c passed all 45 local stages, hosted full run 39714498832 and all 53 associated workflows. PR #425's head d1f17b2d passed all 45 local stages, hosted full run 39823341709 and all 101 associated workflows, and #426's head 9fa07eb4 passed all 45 local stages, hosted full run 39885688347 and all 101 associated workflows. Source fefd91ff stacks on 9fa07eb4 and adds test-first changes, each with its record: the emulated HDA controller and CoreAudio output report poll gaps, stream layout, stale-replay passes and callback timing in new records that change no audio behaviour and leave every B7 evidence file untouched; an ordered-route pointer move behind an awaited receipt replaces the newest unsent move instead of queueing behind every round trip; the existing xHCI HID report pacing is recorded as a QEMU-virt deviation; and live-gate tools no longer echo product output or strand recovery material, refused guest share names are reported, and guest files over the share cap are refused. Each change has a test that failed before it; the shim suites pass 425, 1462 with 3 skipped, 65 and 407 tests. Exact local and hosted checks for this source are pending. These are deterministic checks, not guest behavior; no live run has exercised these changes, and whether audio or pointer smoothness changes is not measured. No final release regression seal is established. See the [audio record](../history/windows-hvf/audio-diagnostics-records-20261022.md), [pointer record](../history/windows-hvf/ordered-pointer-coalescing-20261023.md), [hygiene record](../history/windows-hvf/agent-channel-and-live-gate-hygiene-20261023.md), [replaced A11 notes](../archive/a11-measured-20261023-followups.md), [notes replaced on 2026-10-22](../archive/a11-measured-20261022-robustness.md), [more](../archive/a11-measured-20261022-review.md), [more](../archive/a11-measured-20261022-hardening.md), [more](../archive/a11-measured-20261022-pm.md) and [earlier ones](../archive/a11-measured-20261022.md), [notes for 2026-10-15 to 2026-10-21](../archive/a11-measured-20261015-20261021.md) and [verbatim A11 history](../archive/a11-measured-through-20261014.md).
<!-- END ARCHIVED A11 MEASURED -->
