# A11 observation replaced after the merge of #424 to #427 on 2026-10-23

Classification: historical evidence only. The [registry](../../capabilities/windows-hvf.json)
and [capability matrix](../windows-arm/capability-matrix.md) own current state.
This file preserves, verbatim, an A11 note that a later checkpoint replaced. Copying it
does not revalidate it, close A11 or establish a release result. Earlier observations
are in the [hosted-contract archive](a11-measured-20261023-followups-r5.md) and the
archives it lists.

## Follow-ups seal a9829729, merged to main as 5de1f8ca

Seal `a9829729` recorded this note for source `18be53ea`. It passed all 45 local
stages, hosted full run 39901877995 and all 101 associated workflows. The approved
merge of #427, retargeted to main, made it main `5de1f8ca` with an identical tree,
and main then passed all 45 local stages, hosted full run 39962174688 and all 53
associated workflows. The post-merge integration seal replaced this note. It is
preserved here.

- Source commit: `a9829729ae2c3dff2468652a50447ec2a1cbf4fb` (main `5de1f8cac4664225f71babdb7bc9d67dbfd83df9`).
- Source field: `capabilities/windows-hvf.json`, criterion `A11`, `measured`.
- Source registry SHA-256: `b5cfe4b22e0fc006f2dd08c42ffbfdf1e7784a337891dba17b46585d747da345`.
- Decoded UTF-8 measurement SHA-256: `c021c08d3d5449f9c61498d6708c24e048cdbb9ae4196c618e1e3a6bae78124c`, 2804 bytes.

<!-- BEGIN ARCHIVED A11 MEASURED -->
2026-10-23 04:43 UTC checkpoint: A11 remains OPEN. Main 97de386c passed all 45 local stages, hosted full run 39714498832 and all 53 associated workflows. PR #425's head d1f17b2d passed all 45 local stages, hosted full run 39823341709 and all 101 associated workflows, and #426's head 9fa07eb4 passed all 45 local stages, hosted full run 39885688347 and all 101 associated workflows. Source 18be53ea stacks on 9fa07eb4 and adds test-first changes, each with its record: the emulated HDA controller and CoreAudio output report poll gaps, stream layout, stale-replay passes and callback timing in new records that change no audio behaviour and leave every B7 evidence file untouched; an ordered-route pointer move behind an awaited receipt replaces the newest unsent move instead of queueing behind every round trip; the existing xHCI HID report pacing is recorded as a QEMU-virt deviation; and live-gate tools no longer echo product output or strand recovery material, refused guest share names are reported, and guest files over the share cap are refused. Four earlier seals of this work are superseded and stay recorded: 1c569fb9 failed clippy and 3026230e rustfmt, a hosted run exposed a race in a generation TPM test's scripted swtpm peer, and 23313f25 failed two hand-listed Swift input contracts that check-project.sh does not run; de347fb2, 4da76917, 7e302a8d and 3b58fa97 fixed them. Each change has a test that failed before it; the shim suites pass 425, 1462 with 3 skipped, 65 and 407 tests, and the probe example suite 652. Exact local and hosted checks for this source are pending. These are deterministic checks, not guest behavior; no live run has exercised these changes, and whether audio or pointer smoothness changes is not measured. No final release regression seal is established. See the [audio record](../history/windows-hvf/audio-diagnostics-records-20261022.md), [pointer record](../history/windows-hvf/ordered-pointer-coalescing-20261023.md), [hygiene record](../history/windows-hvf/agent-channel-and-live-gate-hygiene-20261023.md), [replaced A11 notes](../archive/a11-measured-20261023-followups-r5.md), [more](../archive/a11-measured-20261023-followups-r4.md), [more](../archive/a11-measured-20261023-followups-r3.md), [more](../archive/a11-measured-20261023-followups-r2.md), [more](../archive/a11-measured-20261023-followups.md), [notes replaced on 2026-10-22](../archive/a11-measured-20261022-robustness.md), [more](../archive/a11-measured-20261022-review.md), [more](../archive/a11-measured-20261022-hardening.md), [more](../archive/a11-measured-20261022-pm.md) and [earlier ones](../archive/a11-measured-20261022.md), [notes for 2026-10-15 to 2026-10-21](../archive/a11-measured-20261015-20261021.md) and [verbatim A11 history](../archive/a11-measured-through-20261014.md).
<!-- END ARCHIVED A11 MEASURED -->
