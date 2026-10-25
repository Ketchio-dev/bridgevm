# Post-merge integration and live pilots at main 5de1f8ca — 2026-10-23

Classification: the integration of four branches started from main `5de1f8ca`
after the approved merge of #424 to #427, and the live single runs taken at that
main. Each claim is marked with its evidence: a live single run, a host log
observation, an automated test or static reasoning. **No live run has exercised
the integrated changes**, and no criterion or product state changes.

## Live single runs at main 5de1f8ca

These ran the merged main, not this integration. None is claim-eligible, and
none promotes a criterion.

- **T19 r8** (`t19-5de1f8ca-main-import-pilot-r8`, 14:04:46 to 14:29:01Z):
  passed every stage with cleanup verified. Both receipt copies are identical
  and pass the strict verifier; the claim-eligible check refuses, as it must.
  The tier's key accounting read 0 forgotten, 1 absent and 0 refusals.
  (Live single run.) It was the first live run with the per-generation vTPM
  power cycle: every helper start set up its TPM, because a failed setup stops
  the helper and would have failed the run. (Static reasoning from a live
  pass.) A PCR reset is not shown, because the lane keeps no VM log.
- **T17 r91** (`t17-5de1f8ca-main-pilot-r91`, 14:35:14 to 15:17:11Z): passed
  all 17 stages with cleanup verified; receipts identical and strictly valid;
  claim-eligible check refused. (Live single run.) The lane's vTPM key was
  forgotten by the tier: the keychain afterwards held only the operator's own
  item. (Host observation.) A new T19 source was retained and verified.
- **d6 native UI diagnostic** (`bvm-5de1f8ca-native-ui-v2-20261023-01`):
  stopped at its seventh request. The overview press was refused with
  `ax-failure`, AX error -25200 while reading `AXIdentifier` during the
  driver's walk of the main window, 140 ms after the import form was observed.
  All four tripwires stayed 0 and model creations read 0 before and 2 after
  seeding, so the option B invariant held. (Live single run.) The earlier run
  at `97de386c` never reached this point. The cause is not established; a race
  between the walk and the library's re-render after seeding is a hypothesis
  (static reasoning). A separate fix is being prepared and is not part of this
  integration.

## What was integrated

| Branch | Commits (picked) | Record |
| --- | --- | --- |
| hygiene round 2 | `798451a4`, `d48f5cb4`, `d32fbf19` | [round 2](agent-channel-hygiene-r2-20261023.md) |
| guest write policy | `bb5a4652`, `f965da71`, `67c3e370`, `897e4090`, `2365638e`, `a6dff380`, `a52fe830` | [policy](guest-write-policy-20261023.md), [revision](guest-write-policy-revised-20261023.md) |
| T17 TPM PCR evidence | `2c5124d0`, `43f01a22`, `4acc225a`, `0d8a198a` | [TPM evidence](t17-tpm-pcr-evidence-20261023.md) |
| live diagnostic records | `fbabe7df`, `25228362`, `54db7b8a`, `8722b624`, `a1a5e4f7` | [records](live-diagnostic-records-20261023.md) |

Each branch was reviewed against its own red and green logs before it was
picked; the parent reran the probe suite of the guest write branch (683
passed) and the TPM branch's Python, PowerShell and product contracts.

## Where the branches met

- **Two private lane keys.** The TPM branch let both receipt writers ignore a
  private `tpm_evidence` key; the records branch let them admit a well-formed
  `first_run_host_records` key. Each alone refused the other's key. The
  writers now judge the lane keys left after both, and
  `BridgeVMProductE2EMain` nests the records inside the TPM wrapper.
  (Automated test.)
- **Text-scan contracts.** Two contracts pinned the exact source text of the
  older form (`t17-tpm-journey-contract.py` and
  `product-e2e-lane-host-records-test.py`). They now pin the composed form,
  and were folded into the commit that created the conflict so that every
  commit passes on its own tree.
- **New composition tests** (`e1942844`):
  `product-e2e-lane-private-keys-test.py` checks that both writers admit a lane
  with both keys like the plain lane, still refuse malformed records or any
  other unknown key beside them, and copy neither key into a receipt. It fails
  against a writer that drops either key from the check (two red logs kept).
  `ProductE2EPrivateLaneCompositionTests.swift` checks that the nested encoding
  adds exactly both keys to T17 and T19 lanes, and only `tpm_evidence` when the
  host printed no records.
- **Ledgers.** Appended budget rows and manifest includes were kept from both
  sides. One budget row clashed: the guest write branch lowered
  `clip_guest_admission.rs` from 92 to 89 on the same branch, and the lowered
  value was kept. No ceiling rose. `8c4051a4` locks in two extractions from
  the records branch: `ramfb_display_thread.rs` 198 to 176 and
  `machine_assembly.rs` 270 to 267.

## Checks on the integrated tree (`8c4051a4`)

Development feedback only; the hosted run for the sealed head is the required
result.

- rustfmt, both clippy runs, the probe example suite (693 passed) and the HVF
  library (1483 passed, 2 ignored).
- Shim suites: 425, 1462 with 3 skipped, 65 and 436 (ProductE2E rose from 407).
- Both hand-listed Swift contracts outside `check-project.sh` (production input
  driver, input receipt transcript), the reset-record text scan, Python and
  workflow checks, document checks, contract schemas, the product E2E smoke,
  the selected-media contracts, the TPM PowerShell contract and the host-tail
  contracts.
- `rustfmt --check` on the probe files this work touches reports only hunks
  that main already has, file for file (25 files at main, 24 now).

## Registry

- A6 and A7 stay as they are. Their statements remain true, and the renderer
  refuses a `known_defect` on a PROVEN criterion. The new limits are stated in
  the A11 note and in the records above: guest clipboard texts over 8 MiB are
  refused, and guest-written share files are created owner-only with no
  execute bit and a quarantine mark, or refused where the mark cannot be set.
- No live run has exercised either limit, Gatekeeper's handling of the mark,
  or SMB, NFS and FUSE shares.

## Not verified

- No live run at the integrated head: TPM PCR reads in a Windows guest, the
  display timing record from a real exporter, the clipboard cap and the share
  policy are all unobserved.
- That PCR 0 matches across boots after the power-cycle fix is static
  reasoning until the TPM evidence comes back from a live T17 run.
- The d6 overview failure's cause.
