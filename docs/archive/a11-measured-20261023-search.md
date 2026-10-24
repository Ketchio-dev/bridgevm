# A11 observation replaced after the search field change joined the integration on 2026-10-23

Classification: historical evidence only. The [registry](../../capabilities/windows-hvf.json)
and [capability matrix](../windows-arm/capability-matrix.md) own current state.
This file preserves, verbatim, an A11 note that a later checkpoint replaced. Copying it
does not revalidate it, close A11 or establish a release result. Earlier observations
are in the [menu archive](a11-measured-20261023-menu.md) and the archives it lists.

## Integration seal 8d3eb052 (green, then extended)

Seal `8d3eb052` recorded this note for source `158f6a88`. It passed all 45 local
stages, hosted full run 40013060738 and all 103 associated workflows. Among them,
CI's macOS app suites job on macos-15 passed in 17.0 minutes; it also stands for the
same job at `22678cd5`, whose first attempt hit its timeout and whose rerun CI's
concurrency rule cancelled when this seal was pushed. The d6 rerun at this seal then
passed the overview press and stopped at filtering
([search field record](../history/windows-hvf/d6-search-binding-20261023.md)).
Nothing in the note was found wrong; the next seal, which adds the search field
change, replaced it.

- Source commit: `8d3eb052aa3e8016e81d5bc6be9e8fdaed6a60e3`.
- Source field: `capabilities/windows-hvf.json`, criterion `A11`, `measured`.
- Source registry SHA-256: `f774720aeb8863cf542f62eda8b23c82baa9ee5c07e9b3529336262c10112386`.
- Decoded UTF-8 measurement SHA-256: `c061b360f7de40637538884b4d07ad7b834e3d1c72694be114d107c82f2ed1e0`, 3647 bytes.

<!-- BEGIN ARCHIVED A11 MEASURED -->
2026-10-23 19:38 UTC checkpoint: A11 remains OPEN. Main 5de1f8ca, the approved merge of #424 to #427, passed all 45 local stages, hosted full run 39962174688 and all 53 associated workflows. Seals 64b2ecbb and 6121a878 of the post-merge integration each passed all 45 local stages, their hosted full runs 39984341659 and 39995926865, and all 103 associated workflows; seal 22678cd5 passed all 45 local stages, hosted full run 40006503795 and 102 of 103 workflows, the 103rd having hit its 30-minute job timeout in its last step on a slower runner, with its rerun pending. That integration refuses guest clipboard texts over 8 MiB, gives guest-written share files no execute bit and a quarantine mark or refuses them, restarts the native UI driver's read-only accessibility pass within the request deadline while presses stay single-shot, records a content-free locus of the element whose accessibility read failed, and refuses live-queue job ids that receipt redaction would withhold. Source 158f6a88 stacks on seal 22678cd5 and adds one test-first change with its record: the actions menu beside each VM's workspace row and overview card is one shared view that sets an accessibility identifier on the menu itself. Live single runs, not at this source: at 5de1f8ca T19 r8 and T17 r91 passed; at 64b2ecbb T17 r92 passed every lane stage but its receipt was withheld for its job id, and its private lane result shows PCR 0 and PCR 7 identical at all three agent-ready boots; the d6 native UI diagnostic was refused at its overview press with the same AX failure at 5de1f8ca, 6121a878 and 22678cd5, after 109 and 110 restarted passes at the last two, which retracts the transient re-render race assumed at 6121a878, and at 22678cd5 its locus named a menu button that lists its identifier attribute but refuses to read it. Each change has a test that failed before it; the shim suites pass 425, 1464 with 3 skipped, 65 and 436 tests, the probe example suite 693 and the HVF library 1483 with 2 ignored. Exact local and hosted checks for this source are pending. These are deterministic checks, not guest behavior; no live run has exercised the menu identifier. No final release regression seal is established. See the [menu identifier record](../history/windows-hvf/d6-menu-identifier-20261023.md), [locus record](../history/windows-hvf/d6-ax-failure-locus-20261023.md), [integration record](../history/windows-hvf/post-merge-integration-20261023.md), [d6 driver record](../history/windows-hvf/d6-overview-ax-race-20261023.md), [T17 r92 record](../history/windows-hvf/t17-r92-integration-pilot-20261023.md), [guest write policy](../history/windows-hvf/guest-write-policy-revised-20261023.md), [replaced A11 notes](../archive/a11-measured-20261023-menu.md), [more](../archive/a11-measured-20261023-locus.md), [more](../archive/a11-measured-20261023-integration.md), [more](../archive/a11-measured-20261023-merged.md), [more](../archive/a11-measured-20261023-followups-r5.md), [more](../archive/a11-measured-20261023-followups-r4.md), [more](../archive/a11-measured-20261023-followups-r3.md), [more](../archive/a11-measured-20261023-followups-r2.md), [more](../archive/a11-measured-20261023-followups.md), [notes replaced on 2026-10-22](../archive/a11-measured-20261022-robustness.md), [more](../archive/a11-measured-20261022-review.md), [more](../archive/a11-measured-20261022-hardening.md), [more](../archive/a11-measured-20261022-pm.md) and [earlier ones](../archive/a11-measured-20261022.md), [notes for 2026-10-15 to 2026-10-21](../archive/a11-measured-20261015-20261021.md) and [verbatim A11 history](../archive/a11-measured-through-20261014.md).
<!-- END ARCHIVED A11 MEASURED -->
