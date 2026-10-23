# A11 observation replaced after hosted-only contract failures on 2026-10-23

Classification: historical evidence only. The [registry](../../capabilities/windows-hvf.json)
and [capability matrix](../windows-arm/capability-matrix.md) own current state.
This file preserves, verbatim, an A11 note that a later checkpoint replaced. Copying it
does not revalidate it, close A11 or establish a release result. Earlier observations
are in the [flake-fix archive](a11-measured-20261023-followups-r4.md) and the archives
it lists.

## Follow-ups seal 23313f25 (failed two hosted contracts)

Seal `23313f25` recorded this note for source `ee03bb82`. Its local full passed all
45 stages. Two hand-listed Swift contracts outside `check-project.sh` failed for it,
as they had for `3026230e` and `fc28f65c`: the production input driver build, and the
native-to-Swift input receipt transcript in CI's macOS app suites. Neither listed the
file `HvfOrderedInputEvent.swift` that the pointer change created. Commit `3b58fa97`
added it to both lists, and the next seal replaced this note. It is preserved here.

- Source commit: `23313f25111706fabaff87cde62cb114c9f32363`.
- Source field: `capabilities/windows-hvf.json`, criterion `A11`, `measured`.
- Source registry SHA-256: `9699f24f234912378b442704ac3785d1c6df65218415765bd371caf4bf39ed9f`.
- Decoded UTF-8 measurement SHA-256: `ee0b5f9c5994c8daf15c3ac04cd6421fbd7cc02025e9c9d47d83a7db1a012d52`, 2702 bytes.

<!-- BEGIN ARCHIVED A11 MEASURED -->
2026-10-23 04:14 UTC checkpoint: A11 remains OPEN. Main 97de386c passed all 45 local stages, hosted full run 39714498832 and all 53 associated workflows. PR #425's head d1f17b2d passed all 45 local stages, hosted full run 39823341709 and all 101 associated workflows, and #426's head 9fa07eb4 passed all 45 local stages, hosted full run 39885688347 and all 101 associated workflows. Source ee03bb82 stacks on 9fa07eb4 and adds test-first changes, each with its record: the emulated HDA controller and CoreAudio output report poll gaps, stream layout, stale-replay passes and callback timing in new records that change no audio behaviour and leave every B7 evidence file untouched; an ordered-route pointer move behind an awaited receipt replaces the newest unsent move instead of queueing behind every round trip; the existing xHCI HID report pacing is recorded as a QEMU-virt deviation; and live-gate tools no longer echo product output or strand recovery material, refused guest share names are reported, and guest files over the share cap are refused. Earlier seals of this work stay recorded: 1c569fb9 failed clippy and 3026230e failed rustfmt (fixed by de347fb2 and 4da76917), and a hosted run of 3026230e exposed a race in a generation TPM test's scripted swtpm peer, fixed by 7e302a8d; fc28f65c passed all 45 local stages before that fix. Each change has a test that failed before it; the shim suites pass 425, 1462 with 3 skipped, 65 and 407 tests, and the probe example suite 652. Exact local and hosted checks for this source are pending. These are deterministic checks, not guest behavior; no live run has exercised these changes, and whether audio or pointer smoothness changes is not measured. No final release regression seal is established. See the [audio record](../history/windows-hvf/audio-diagnostics-records-20261022.md), [pointer record](../history/windows-hvf/ordered-pointer-coalescing-20261023.md), [hygiene record](../history/windows-hvf/agent-channel-and-live-gate-hygiene-20261023.md), [replaced A11 notes](../archive/a11-measured-20261023-followups-r4.md), [more](../archive/a11-measured-20261023-followups-r3.md), [more](../archive/a11-measured-20261023-followups-r2.md), [more](../archive/a11-measured-20261023-followups.md), [notes replaced on 2026-10-22](../archive/a11-measured-20261022-robustness.md), [more](../archive/a11-measured-20261022-review.md), [more](../archive/a11-measured-20261022-hardening.md), [more](../archive/a11-measured-20261022-pm.md) and [earlier ones](../archive/a11-measured-20261022.md), [notes for 2026-10-15 to 2026-10-21](../archive/a11-measured-20261015-20261021.md) and [verbatim A11 history](../archive/a11-measured-through-20261014.md).
<!-- END ARCHIVED A11 MEASURED -->
