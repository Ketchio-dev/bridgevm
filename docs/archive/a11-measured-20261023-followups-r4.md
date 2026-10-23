# A11 observation replaced after a hosted-only flake on 2026-10-23

Classification: historical evidence only. The [registry](../../capabilities/windows-hvf.json)
and [capability matrix](../windows-arm/capability-matrix.md) own current state.
This file preserves, verbatim, an A11 note that a later checkpoint replaced. Copying it
does not revalidate it, close A11 or establish a release result. Earlier observations
are in the [second failed-seal archive](a11-measured-20261023-followups-r3.md) and the
archives it lists.

## Follow-ups seal fc28f65c (superseded by a test-harness fix)

Seal `fc28f65c` recorded this note for source `31f573fa`. Its local full passed all
45 stages. Hosted run 39895178899 for the earlier seal `3026230e` also failed a
probe example test, besides rustfmt. A scripted swtpm peer in the generation TPM
tests panicked on EINVAL from `set_read_timeout`, because the client had already
closed the connection. Commit `7e302a8d` fixed that test-harness race, and the next
seal replaced this note. It is preserved here.

- Source commit: `fc28f65cc0fa4702fcc2b33b4688a1a359138237`.
- Source field: `capabilities/windows-hvf.json`, criterion `A11`, `measured`.
- Source registry SHA-256: `1df41c8fc26ac61dde50d0dafbee507e8195a759d129bcb907ded7e9ca68e4a6`.
- Decoded UTF-8 measurement SHA-256: `a970ee43c4b26ae392f382d855ce7d943870e2775d158e512db840b0d9574aca`, 2566 bytes.

<!-- BEGIN ARCHIVED A11 MEASURED -->
2026-10-23 03:50 UTC checkpoint: A11 remains OPEN. Main 97de386c passed all 45 local stages, hosted full run 39714498832 and all 53 associated workflows. PR #425's head d1f17b2d passed all 45 local stages, hosted full run 39823341709 and all 101 associated workflows, and #426's head 9fa07eb4 passed all 45 local stages, hosted full run 39885688347 and all 101 associated workflows. Source 31f573fa stacks on 9fa07eb4 and adds test-first changes, each with its record: the emulated HDA controller and CoreAudio output report poll gaps, stream layout, stale-replay passes and callback timing in new records that change no audio behaviour and leave every B7 evidence file untouched; an ordered-route pointer move behind an awaited receipt replaces the newest unsent move instead of queueing behind every round trip; the existing xHCI HID report pacing is recorded as a QEMU-virt deviation; and live-gate tools no longer echo product output or strand recovery material, refused guest share names are reported, and guest files over the share cap are refused. Two earlier seals of this work failed their local full and stay recorded: 1c569fb9 on clippy, from a new test written before #426 changed the platform reset, and 3026230e on rustfmt, from the line that fixed it; de347fb2 and 4da76917 fixed them. Each change has a test that failed before it; the shim suites pass 425, 1462 with 3 skipped, 65 and 407 tests. Exact local and hosted checks for this source are pending. These are deterministic checks, not guest behavior; no live run has exercised these changes, and whether audio or pointer smoothness changes is not measured. No final release regression seal is established. See the [audio record](../history/windows-hvf/audio-diagnostics-records-20261022.md), [pointer record](../history/windows-hvf/ordered-pointer-coalescing-20261023.md), [hygiene record](../history/windows-hvf/agent-channel-and-live-gate-hygiene-20261023.md), [replaced A11 notes](../archive/a11-measured-20261023-followups-r3.md), [more](../archive/a11-measured-20261023-followups-r2.md), [more](../archive/a11-measured-20261023-followups.md), [notes replaced on 2026-10-22](../archive/a11-measured-20261022-robustness.md), [more](../archive/a11-measured-20261022-review.md), [more](../archive/a11-measured-20261022-hardening.md), [more](../archive/a11-measured-20261022-pm.md) and [earlier ones](../archive/a11-measured-20261022.md), [notes for 2026-10-15 to 2026-10-21](../archive/a11-measured-20261015-20261021.md) and [verbatim A11 history](../archive/a11-measured-through-20261014.md).
<!-- END ARCHIVED A11 MEASURED -->
