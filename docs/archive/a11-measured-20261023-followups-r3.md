# A11 observation replaced after the second failed follow-ups seal on 2026-10-23

Classification: historical evidence only. The [registry](../../capabilities/windows-hvf.json)
and [capability matrix](../windows-arm/capability-matrix.md) own current state.
This file preserves, verbatim, an A11 note that a later checkpoint replaced. Copying it
does not revalidate it, close A11 or establish a release result. Earlier observations
are in the [first failed-seal archive](a11-measured-20261023-followups-r2.md), the
[follow-ups archive](a11-measured-20261023-followups.md) and the archives they list.

## Follow-ups seal 3026230e (failed its local full)

Seal `3026230e` recorded this note for source `c7c4f801`. Its local full failed
`rustfmt`: the line `de347fb2` added to fix the first failed seal exceeded rustfmt's
chain width. Every other stage passed, including both clippy steps. Commit `4da76917`
fixed the line within the file's structural budget, and the next seal replaced this
note. It is preserved here.

- Source commit: `3026230e36bd3bfe0ce30c7bb16b389a8da7bb55`.
- Source field: `capabilities/windows-hvf.json`, criterion `A11`, `measured`.
- Source registry SHA-256: `f728457d338f6bcb96d47bb063ff9905f571b9477f6ee194c6603be0ea25a755`.
- Decoded UTF-8 measurement SHA-256: `893b81f5dc58888a6d5075266fb6e05d6aaea242b89655946be2e691acbc5f01`, 2429 bytes.

<!-- BEGIN ARCHIVED A11 MEASURED -->
2026-10-23 03:27 UTC checkpoint: A11 remains OPEN. Main 97de386c passed all 45 local stages, hosted full run 39714498832 and all 53 associated workflows. PR #425's head d1f17b2d passed all 45 local stages, hosted full run 39823341709 and all 101 associated workflows, and #426's head 9fa07eb4 passed all 45 local stages, hosted full run 39885688347 and all 101 associated workflows. Source c7c4f801 stacks on 9fa07eb4 and adds test-first changes, each with its record: the emulated HDA controller and CoreAudio output report poll gaps, stream layout, stale-replay passes and callback timing in new records that change no audio behaviour and leave every B7 evidence file untouched; an ordered-route pointer move behind an awaited receipt replaces the newest unsent move instead of queueing behind every round trip; the existing xHCI HID report pacing is recorded as a QEMU-virt deviation; and live-gate tools no longer echo product output or strand recovery material, refused guest share names are reported, and guest files over the share cap are refused. The earlier seal 1c569fb9 failed its local full: clippy rejected an unused Result in a new test written before #426 changed the platform reset, and de347fb2 fixed it. Each change has a test that failed before it; the shim suites pass 425, 1462 with 3 skipped, 65 and 407 tests. Exact local and hosted checks for this source are pending. These are deterministic checks, not guest behavior; no live run has exercised these changes, and whether audio or pointer smoothness changes is not measured. No final release regression seal is established. See the [audio record](../history/windows-hvf/audio-diagnostics-records-20261022.md), [pointer record](../history/windows-hvf/ordered-pointer-coalescing-20261023.md), [hygiene record](../history/windows-hvf/agent-channel-and-live-gate-hygiene-20261023.md), [replaced A11 notes](../archive/a11-measured-20261023-followups-r2.md), [more](../archive/a11-measured-20261023-followups.md), [notes replaced on 2026-10-22](../archive/a11-measured-20261022-robustness.md), [more](../archive/a11-measured-20261022-review.md), [more](../archive/a11-measured-20261022-hardening.md), [more](../archive/a11-measured-20261022-pm.md) and [earlier ones](../archive/a11-measured-20261022.md), [notes for 2026-10-15 to 2026-10-21](../archive/a11-measured-20261015-20261021.md) and [verbatim A11 history](../archive/a11-measured-through-20261014.md).
<!-- END ARCHIVED A11 MEASURED -->
