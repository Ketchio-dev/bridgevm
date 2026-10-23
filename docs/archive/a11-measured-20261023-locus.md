# A11 observation replaced after the d6 failure locus joined the integration on 2026-10-23

Classification: historical evidence only. The [registry](../../capabilities/windows-hvf.json)
and [capability matrix](../windows-arm/capability-matrix.md) own current state.
This file preserves, verbatim, an A11 note that a later checkpoint replaced. Copying it
does not revalidate it, close A11 or establish a release result. Earlier observations
are in the [integration archive](a11-measured-20261023-integration.md) and the archives it lists.

## Integration seal 6121a878 (green, then extended)

Seal `6121a878` recorded this note for source `00fed86f`. It passed all 45 local
stages, hosted full run 39995926865 and all 103 associated workflows. The d6 rerun at
this seal was then refused at its overview press again, after 109 restarted read-only
passes. That retracted the transient re-render race the driver's restart change had
assumed ([locus record](../history/windows-hvf/d6-ax-failure-locus-20261023.md)). The
note describes that change as code and claims no live effect, so its statements stand;
the next seal, which adds the failure locus, replaced it.

- Source commit: `6121a878deb4cb638ae838de903e2499de2b6a57`.
- Source field: `capabilities/windows-hvf.json`, criterion `A11`, `measured`.
- Source registry SHA-256: `1ac127a5f670b48011f833a3b10ae4f1f8ab9e52c5db35f9cf7e44d5093826a5`.
- Decoded UTF-8 measurement SHA-256: `6e6b2e8a4fb1cd3ad6f8b03e4c0f7a66fdf07e5ecc89c80002caffb49e12f9c5`, 2886 bytes.

<!-- BEGIN ARCHIVED A11 MEASURED -->
2026-10-23 17:31 UTC checkpoint: A11 remains OPEN. Main 5de1f8ca, the approved merge of #424 to #427, passed all 45 local stages, hosted full run 39962174688 and all 53 associated workflows. Seal 64b2ecbb of the post-merge integration passed all 45 local stages, hosted full run 39984341659 and all 103 associated workflows; that integration refuses guest clipboard texts over 8 MiB and gives guest-written share files no execute bit and a quarantine mark, or refuses them. Source 00fed86f stacks on that seal and adds test-first changes, each with its record: the native UI driver restarts its read-only accessibility pass after a transient AX status within the request deadline while presses stay single-shot, and records how often it did; the UI host requests the overview press only once both seeded rows exist; and live-queue submit refuses a job id that receipt redaction would withhold. Live single runs, not at this source: at 5de1f8ca T19 r8 and T17 r91 passed, and the d6 native UI diagnostic was refused at its overview press with an AX failure; at 64b2ecbb T17 r92 passed every lane stage but its receipt was withheld because its job id contained a vTPM word, and its private lane result shows PCR 0 and PCR 7 identical at all three agent-ready boots, including a read after a guest restart. Each change has a test that failed before it; the shim suites pass 425, 1462 with 3 skipped, 65 and 436 tests, the probe example suite 693 and the HVF library 1483 with 2 ignored. Exact local and hosted checks for this source are pending. These are deterministic checks, not guest behavior; no live run has exercised the driver or submit changes. No final release regression seal is established. See the [integration record](../history/windows-hvf/post-merge-integration-20261023.md), [d6 driver record](../history/windows-hvf/d6-overview-ax-race-20261023.md), [T17 r92 record](../history/windows-hvf/t17-r92-integration-pilot-20261023.md), [guest write policy](../history/windows-hvf/guest-write-policy-revised-20261023.md), [replaced A11 notes](../archive/a11-measured-20261023-integration.md), [more](../archive/a11-measured-20261023-merged.md), [more](../archive/a11-measured-20261023-followups-r5.md), [more](../archive/a11-measured-20261023-followups-r4.md), [more](../archive/a11-measured-20261023-followups-r3.md), [more](../archive/a11-measured-20261023-followups-r2.md), [more](../archive/a11-measured-20261023-followups.md), [notes replaced on 2026-10-22](../archive/a11-measured-20261022-robustness.md), [more](../archive/a11-measured-20261022-review.md), [more](../archive/a11-measured-20261022-hardening.md), [more](../archive/a11-measured-20261022-pm.md) and [earlier ones](../archive/a11-measured-20261022.md), [notes for 2026-10-15 to 2026-10-21](../archive/a11-measured-20261015-20261021.md) and [verbatim A11 history](../archive/a11-measured-through-20261014.md).
<!-- END ARCHIVED A11 MEASURED -->
