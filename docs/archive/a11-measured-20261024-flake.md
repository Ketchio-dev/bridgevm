# A11 observation replaced after the 1db70ab8 hosted full failed a timing-sensitive test on 2026-10-24

Classification: historical evidence only. The [registry](../../capabilities/windows-hvf.json)
and [capability matrix](../windows-arm/capability-matrix.md) own current state.
This file preserves, verbatim, an A11 note that a later checkpoint replaced. Copying it
does not revalidate it, close A11 or establish a release result. Earlier observations
are in the [r95 archive](a11-measured-20261024-r95.md) and the archives it lists.

## Integration seal 1db70ab8 (local green, hosted red)

Seal `1db70ab8` recorded this note for source `065c8b6f`. It passed all 45 local
stages and 102 of its 103 associated workflows; hosted full run 40056629747 failed one
probe example test whose bound assumed that a 10 ms sleep never stretches to the
100 ms deadline
([integration record, round 7](../history/windows-hvf/followups-r2-integration-r7-20261024.md)).
The next seal, with the corrected test, replaced the note.

- Source commit: `1db70ab8e7c1a4174ec45c374be4b4c51cb853f4`.
- Source field: `capabilities/windows-hvf.json`, criterion `A11`, `measured`.
- Source registry SHA-256: `11cfc63f7cf3723ca00f7bc49ff78d5211e3c6cc69bd91e2e46140abcae054ad`.
- Decoded UTF-8 measurement SHA-256: `fe0a0fbb34db6680f0eb584be8e67e7968dcaeb4c6bb6dcaafc19c115d3ea0bd`, 3023 bytes.

<!-- BEGIN ARCHIVED A11 MEASURED -->
2026-10-24 03:01 UTC checkpoint: A11 remains OPEN. Main 5de1f8ca, the approved merge of #424 to #427, passed all 45 local stages, hosted full run 39962174688 and all 53 associated workflows. The post-merge integration's seals 64b2ecbb, 6121a878, 8d3eb052, 493e1bbe, 80733f99, 79da0472 and 43007ca0 each passed all 45 local stages, their hosted full runs and all 103 associated workflows; seal 22678cd5 passed 102 of 103, its 103rd job having hit its 30-minute timeout on a slower runner and passed at the next seal. That integration refuses guest clipboard texts over 8 MiB, gives guest-written share files and directories owner-only modes and a quarantine mark or refuses them, prints only a length and hash prefix for operator clipboard reads, gives native UI driver refusals a content-free locus, gives the VM actions menus and the library search field working accessibility identifiers, refuses live-queue job ids that receipt redaction would withhold, adds an opt-in T19 BitLocker restart check that never creates recovery material, advances HDA playback DMA from a 5 ms host service tick recorded as a QEMU-virt deviation, pulls each 3D-off display export from the app's display tick, and withdrew a 70 ms CoreAudio ring high-water mark under a test that pins A5 in a capped poll gap. Source 065c8b6f adds only records to seal 43007ca0. Live single runs, none at this source: at 5de1f8ca T19 r8 and T17 r91 passed; at 64b2ecbb T17 r92 passed every lane stage but its receipt was withheld for its job id; the d6 native UI diagnostic was refused at its overview press at 5de1f8ca, 6121a878 and 22678cd5, stopped at filtering at 8d3eb052, and completed all 12 requests at 493e1bbe; at 80733f99 T19 r9 passed, its opt-in BitLocker check refused because the guest edition is Windows 11 Home; at 79da0472 T17 r94 failed its audio stage with 275 drops after the high-water mark trimmed audio following capped poll gaps; at 43007ca0, whose code this source keeps, T17 r95 passed every lane stage with zero audio drops, no capped poll gap, a 23 to 58 ms CoreAudio ring and 98% of display export wakes within 1 ms. Each change has a test that failed before it; the shim suites pass 430, 1469 with 3 skipped, 65 and 454 tests, the probe example suite 727 and the HVF library 1493 with 2 ignored. Exact local and hosted checks for this source are pending. These counts are deterministic checks, not guest behavior; single live runs are not gate samples, and none has yet had a capped poll gap since the withdrawal. No final release regression seal is established. See the [round 6 integration record](../history/windows-hvf/followups-r2-integration-r6-20261024.md), [round 5 integration record](../history/windows-hvf/followups-r2-integration-r5-20261024.md), [withdrawal record](../history/windows-hvf/hda-mark-withdrawal-20261024.md), [integration record](../history/windows-hvf/post-merge-integration-20261023.md) and [replaced A11 notes](../archive/a11-measured-20261024-r95.md), whose archive chain lists every earlier note.
<!-- END ARCHIVED A11 MEASURED -->
