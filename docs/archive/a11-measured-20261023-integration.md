# A11 observation replaced after the d6 driver fix joined the integration on 2026-10-23

Classification: historical evidence only. The [registry](../../capabilities/windows-hvf.json)
and [capability matrix](../windows-arm/capability-matrix.md) own current state.
This file preserves, verbatim, an A11 note that a later checkpoint replaced. Copying it
does not revalidate it, close A11 or establish a release result. Earlier observations
are in the [merge archive](a11-measured-20261023-merged.md) and the archives it lists.

## Post-merge integration seal 64b2ecbb (green, then extended)

Seal `64b2ecbb` recorded this note for source `745aa217`. It passed all 45 local
stages, hosted full run 39984341659 and all 103 associated workflows. The d6 driver
fix for the overview AX refusal seen live at main `5de1f8ca` was then stacked on it,
and the next seal replaced this note. Nothing in it was found wrong; it is preserved
here.

- Source commit: `64b2ecbbb6ed34e2a3f5e96ce824791ebbc0c8e1`.
- Source field: `capabilities/windows-hvf.json`, criterion `A11`, `measured`.
- Source registry SHA-256: `8e457a3eb796155500bd3bc73006f50250aeaf716376055d4acc466e6a83c515`.
- Decoded UTF-8 measurement SHA-256: `a6e8847cf275c9c032d7797e17628925838d4615a9ec2d6d5c8798a292867c7c`, 2948 bytes.

<!-- BEGIN ARCHIVED A11 MEASURED -->
2026-10-23 16:07 UTC checkpoint: A11 remains OPEN. Main 5de1f8ca, the approved merge of #424 to #427, passed all 45 local stages, hosted full run 39962174688 and all 53 associated workflows. Source 745aa217 stacks on 5de1f8ca and adds test-first changes, each with its record: harness-mode GETEND completes only the GET its GETBEG began and never writes through a leaf symlink, and the T19 source handoff prints only fixed refusal text; guest clipboard texts over 8 MiB are refused before they reach the Mac pasteboard, and guest-written share files are created owner-only with no execute bit and a quarantine mark, or refused where the mark cannot be set, with msdos and exfat volumes keeping their forced execute bits only under a mark that reads back; the T17 and T19 journeys read PCR 0 and 7 through TBS at each agent-ready boot into a private lane field that never decides a lane; the first run's HDA records and a new ramfb display timing record are carried in the private lane results; and the composite HID device is recorded as a QEMU-virt deviation. Each change has a test that failed before it; the shim suites pass 425, 1462 with 3 skipped, 65 and 436 tests, the probe example suite 693 and the HVF library 1483 with 2 ignored. Live single runs at main 5de1f8ca, not at this source: T19 r8 and T17 r91 passed, and the d6 native UI diagnostic was refused at its overview press with an AX failure while every tripwire stayed 0. Exact local and hosted checks for this source are pending. These are deterministic checks, not guest behavior; no live run has exercised these changes, and the clipboard and share limits, the TPM reads and the display record are unobserved in a guest. No final release regression seal is established. See the [integration record](../history/windows-hvf/post-merge-integration-20261023.md), [guest write policy](../history/windows-hvf/guest-write-policy-revised-20261023.md), [TPM evidence](../history/windows-hvf/t17-tpm-pcr-evidence-20261023.md), [diagnostic records](../history/windows-hvf/live-diagnostic-records-20261023.md), [hygiene round 2](../history/windows-hvf/agent-channel-hygiene-r2-20261023.md), [replaced A11 notes](../archive/a11-measured-20261023-merged.md), [more](../archive/a11-measured-20261023-followups-r5.md), [more](../archive/a11-measured-20261023-followups-r4.md), [more](../archive/a11-measured-20261023-followups-r3.md), [more](../archive/a11-measured-20261023-followups-r2.md), [more](../archive/a11-measured-20261023-followups.md), [notes replaced on 2026-10-22](../archive/a11-measured-20261022-robustness.md), [more](../archive/a11-measured-20261022-review.md), [more](../archive/a11-measured-20261022-hardening.md), [more](../archive/a11-measured-20261022-pm.md) and [earlier ones](../archive/a11-measured-20261022.md), [notes for 2026-10-15 to 2026-10-21](../archive/a11-measured-20261015-20261021.md) and [verbatim A11 history](../archive/a11-measured-through-20261014.md).
<!-- END ARCHIVED A11 MEASURED -->
