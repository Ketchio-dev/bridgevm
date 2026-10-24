# A11 observation replaced after the actions menu identifier joined the integration on 2026-10-23

Classification: historical evidence only. The [registry](../../capabilities/windows-hvf.json)
and [capability matrix](../windows-arm/capability-matrix.md) own current state.
This file preserves, verbatim, an A11 note that a later checkpoint replaced. Copying it
does not revalidate it, close A11 or establish a release result. Earlier observations
are in the [locus archive](a11-measured-20261023-locus.md) and the archives it lists.

## Integration seal 22678cd5 (green but one timed-out job, then extended)

Seal `22678cd5` recorded this note for source `d53d479f`. It passed all 45 local
stages, hosted full run 40006503795 and 102 of its 103 associated workflows.
The 103rd, CI's macOS app suites job on macos-15 (run 40006511489), reached its
last step and then hit its 30-minute job timeout on a slower runner, where every
step took about half again as long as at seal `6121a878`; that job was rerun as
attempt 2. The d6 rerun at this seal then named
the failing element through the locus this note describes
([menu identifier record](../history/windows-hvf/d6-menu-identifier-20261023.md)).
Nothing in the note was found wrong; the next seal, which adds the actions menu
identifier, replaced it.

- Source commit: `22678cd56eec076ddfdfa508b79166c7e4e4664c`.
- Source field: `capabilities/windows-hvf.json`, criterion `A11`, `measured`.
- Source registry SHA-256: `dd57e14f8aff3607d8e88cadc115d63faae59ca8103c03d17e6906ea97cfa0ef`.
- Decoded UTF-8 measurement SHA-256: `ba18b7b71a139219d8bce3d9a6479828c93765a295ea36fe7695e92000a96f28`, 3322 bytes.

<!-- BEGIN ARCHIVED A11 MEASURED -->
2026-10-23 18:48 UTC checkpoint: A11 remains OPEN. Main 5de1f8ca, the approved merge of #424 to #427, passed all 45 local stages, hosted full run 39962174688 and all 53 associated workflows. Seals 64b2ecbb and 6121a878 of the post-merge integration each passed all 45 local stages, their hosted full runs 39984341659 and 39995926865, and all 103 associated workflows; that integration refuses guest clipboard texts over 8 MiB, gives guest-written share files no execute bit and a quarantine mark or refuses them, restarts the native UI driver's read-only accessibility pass within the request deadline while presses stay single-shot, and refuses live-queue job ids that receipt redaction would withhold. Source d53d479f stacks on seal 6121a878 and adds one test-first change with its record: a d6 driver refusal raised inside that read-only pass carries a content-free locus of the failing element, its depth and child index path, its role, subrole and parent role, its attribute names and how many distinct positions failed across the restarts, and every reply validator refuses a malformed locus. Live single runs, not at this source: at 5de1f8ca T19 r8 and T17 r91 passed; at 64b2ecbb T17 r92 passed every lane stage but its receipt was withheld for its job id, and its private lane result shows PCR 0 and PCR 7 identical at all three agent-ready boots; at 5de1f8ca and again at 6121a878 the d6 native UI diagnostic was refused at its overview press with the same AX failure, the second time after 109 restarted passes, so the transient re-render race the restart change assumed is retracted and the failing element is unidentified. Each change has a test that failed before it; the shim suites pass 425, 1462 with 3 skipped, 65 and 436 tests, the probe example suite 693 and the HVF library 1483 with 2 ignored. Exact local and hosted checks for this source are pending. These are deterministic checks, not guest behavior; no live run has exercised the locus. No final release regression seal is established. See the [locus record](../history/windows-hvf/d6-ax-failure-locus-20261023.md), [integration record](../history/windows-hvf/post-merge-integration-20261023.md), [d6 driver record](../history/windows-hvf/d6-overview-ax-race-20261023.md), [T17 r92 record](../history/windows-hvf/t17-r92-integration-pilot-20261023.md), [guest write policy](../history/windows-hvf/guest-write-policy-revised-20261023.md), [replaced A11 notes](../archive/a11-measured-20261023-locus.md), [more](../archive/a11-measured-20261023-integration.md), [more](../archive/a11-measured-20261023-merged.md), [more](../archive/a11-measured-20261023-followups-r5.md), [more](../archive/a11-measured-20261023-followups-r4.md), [more](../archive/a11-measured-20261023-followups-r3.md), [more](../archive/a11-measured-20261023-followups-r2.md), [more](../archive/a11-measured-20261023-followups.md), [notes replaced on 2026-10-22](../archive/a11-measured-20261022-robustness.md), [more](../archive/a11-measured-20261022-review.md), [more](../archive/a11-measured-20261022-hardening.md), [more](../archive/a11-measured-20261022-pm.md) and [earlier ones](../archive/a11-measured-20261022.md), [notes for 2026-10-15 to 2026-10-21](../archive/a11-measured-20261015-20261021.md) and [verbatim A11 history](../archive/a11-measured-through-20261014.md).
<!-- END ARCHIVED A11 MEASURED -->
