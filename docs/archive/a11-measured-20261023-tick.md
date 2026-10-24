# A11 observation replaced after the HDA tick and the pulled display export joined the integration on 2026-10-23

Classification: historical evidence only. The [registry](../../capabilities/windows-hvf.json)
and [capability matrix](../windows-arm/capability-matrix.md) own current state.
This file preserves, verbatim, an A11 note that a later checkpoint replaced. Copying it
does not revalidate it, close A11 or establish a release result. Earlier observations
are in the [d6 pass archive](a11-measured-20261023-d6pass.md) and the archives it lists.

## Integration seal 80733f99 (green, then extended)

Seal `80733f99` recorded this note for source `07c1f1e9`. It passed all 45 local
stages, hosted full run 40036162820 and all 103 associated workflows. T19 r9 at this
seal then passed, with its opt-in BitLocker check refused by the guest's edition
([integration record, round 4](../history/windows-hvf/followups-r2-integration-r4-20261023.md)).
Nothing in the note was found wrong; the next seal, which adds the HDA service tick,
the ring high-water mark and the pulled display export, replaced it.

- Source commit: `80733f9959532ee3ceaacf0e59d91b513ce10cc8`.
- Source field: `capabilities/windows-hvf.json`, criterion `A11`, `measured`.
- Source registry SHA-256: `5086dc7ac6308de561546d4023c7d34b791d6ef5f5d8a12d53b7fceda9cf3d59`.
- Decoded UTF-8 measurement SHA-256: `95eed810ac9e3d974b8cdb233672c4f9894dccf0662005bcf1a893bc8da77cd8`, 3002 bytes.

<!-- BEGIN ARCHIVED A11 MEASURED -->
2026-10-23 22:50 UTC checkpoint: A11 remains OPEN. Main 5de1f8ca, the approved merge of #424 to #427, passed all 45 local stages, hosted full run 39962174688 and all 53 associated workflows. The post-merge integration's seals 64b2ecbb, 6121a878, 8d3eb052 and 493e1bbe each passed all 45 local stages, their hosted full runs and all 103 associated workflows; seal 22678cd5 passed 102 of 103, its 103rd job having hit its 30-minute timeout on a slower runner and passed at the next seal. That integration refuses guest clipboard texts over 8 MiB, gives guest-written share files no execute bit and a quarantine mark or refuses them, adds a content-free locus to native UI driver accessibility refusals, gives the VM actions menus an accessibility identifier, lets an accessibility value set on the library search field filter the list, and refuses live-queue job ids that receipt redaction would withhold. Source 07c1f1e9 stacks on seal 493e1bbe and adds test-first changes with their records: the guest-tools copy button reads the daemon's clipboard field; an operator guest clipboard read prints only the text's byte length and a SHA-256 prefix; a guest file the write policy refused for good is not fetched again until it changes; directories created for guest files are owner-only and quarantine-marked or refused; and an opt-in T19 BitLocker restart check, enabled only by one sealed pilot manifest row, adds only a TPM protector, never creates recovery material and records private codes only, while default T17 and T19 inputs stay byte-identical. Live single runs, not at this source: at 5de1f8ca T19 r8 and T17 r91 passed; at 64b2ecbb T17 r92 passed every lane stage but its receipt was withheld for its job id, and its private lane result shows PCR 0 and PCR 7 identical at all three agent-ready boots; the d6 native UI diagnostic was refused at its overview press at 5de1f8ca, 6121a878 and 22678cd5, stopped at filtering at 8d3eb052, and completed all 12 requests at 493e1bbe. The installed guest edition is Windows 11 Home, which offers no BitLocker, so on current sources the restart check can only refuse. Each change has a test that failed before it; the shim suites pass 430, 1465 with 3 skipped, 65 and 454 tests, the probe example suite 708 and the HVF library 1483 with 2 ignored. Exact local and hosted checks for this source are pending. These are deterministic checks, not guest behavior. No final release regression seal is established. See the [round 3 integration record](../history/windows-hvf/followups-r2-integration-r3-20261023.md), [guest-write follow-ups](../history/windows-hvf/guest-write-followups-20261023.md), [BitLocker restart check](../history/windows-hvf/bitlocker-restart-check-20261023.md), [search field record](../history/windows-hvf/d6-search-binding-20261023.md), [integration record](../history/windows-hvf/post-merge-integration-20261023.md) and [replaced A11 notes](../archive/a11-measured-20261023-d6pass.md), whose archive chain lists every earlier note.
<!-- END ARCHIVED A11 MEASURED -->
