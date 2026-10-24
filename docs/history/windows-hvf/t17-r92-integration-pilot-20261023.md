# T17 r92 at the integration seal: TPM PCR evidence and first-run records — 2026-10-23

Classification: one live single run of the T17 product journey at seal `64b2ecbb`
(draft PR #428). Each claim is marked with its evidence: a live single run, a host
observation, an automated test or static reasoning. The run is nonpromoting and
not claim-eligible, and no criterion or product state changes.

## Result: FAIL, receipt withheld

- Job `t17-64b2ecbb-integration-tpm-pilot-r92` ran 16:32:29 to 17:11:56Z and
  ended `result=fail`, `exit_code=1`, `receipt=withheld`. (Live single run.)
- The lane itself passed. The private lane result has every one of the 17 stage
  booleans true, failure code `none` and cleanup verified. The private receipt
  reads `pass: true` with one pass and no failures, and the strict verifier
  accepts it at the exact commit; the claim-eligible check refuses it, as it
  must. (Host observation of the private outputs.)
- Publication failed: `redact-receipt.py` refused the receipt because its
  `job_id` matched the `\b(tpm|vtpm|swtpm)\b` pattern. The id was a naming
  mistake made at submission; the redactor behaved as designed and failed
  closed. The worker then marked the job failed. Nothing was published.
- `6cb30c6d` makes `bridgevm-live submit` refuse such an id before burning it,
  using the redactor's own patterns. (Automated test, red before and green after.)
- The lane's vTPM key was forgotten: the keychain afterwards held only the
  operator's own item. (Host observation.) A 17.6 GiB T19 source was retained
  and verified, then deleted to keep the live volume above its 100 GiB admission
  floor; the source from the passing r91 run was kept.

## TPM PCR evidence (private `tpm_evidence`)

| Boot | Status | TPM present / ready | PCR 0 SHA-256 | PCR 7 SHA-256 | PCR 0 equals boot 1 | Guest restart before read |
| --- | --- | --- | --- | --- | --- | --- |
| 1 | available | true / true | `f56d6d9a…` | `bd1f6e22…` | true | true |
| 2 | available | true / true | `f56d6d9a…` | `bd1f6e22…` | true | false |
| 3 | available | true / true | `f56d6d9a…` | `bd1f6e22…` | true | false |

- The guest read PCR 0 and 7 through TBS at all three agent-ready boots, with
  `Get-Tpm` reporting the TPM present and ready each time. (Live single run.)
- Boot 1 includes a guest restart before its read, and its PCR 0 equals that of
  boots 2 and 3. (Live single run.) In product mode that restart respawns the
  helper against the same swtpm, while each later boot starts a fresh swtpm
  process ([TPM evidence record](t17-tpm-pcr-evidence-20261023.md)). (Static
  reasoning.)
- Reading: had the respawned helper not power-cycled swtpm, firmware would have
  extended PCR 0 a second time and boot 1 would differ. The match is what the
  per-generation power cycle (`acae6e96`, on main as `e88979cd`; see
  [the TPM failure record](tpm-backend-failure-20261022.md)) predicts. (Static
  reasoning from one live run.)
- **Not exercised:** BitLocker. No guest volume was encrypted, so no unseal
  against PCR 7 or PCR 0 was attempted.

## First-run host records (private `first_run_host_records`)

All five records were printed and carried; values are from the first boot only,
which plays audio and takes the keyboard and pointer challenge. (Live single run.)

- `hda_stream`: 2 runs, 48 kHz stereo 16-bit, a 20 ms cyclic buffer in two 10 ms
  entries, 5,807 ms running.
- `hda_poll`: 26,987 polls; 26,802 gaps of 10 ms or less, one over 100 ms
  (216 ms; one capped poll lost 116 ms); 568 IOC completions raised 372 MSIs.
- `hda_repeat`: 568 passes, 289 silent, 277 compared, 30 repeats of the previous
  pass (stale replays), never two in a row.
- `hda_coreaudio_timing`: 19,297 callbacks, all within 15 ms (maximum 11 ms),
  none late, no trylock misses; the output ring ranged 0 to 114 ms and ended at
  104 ms.
- `ramfb_display_timing`: 5,837 exporter ticks woke late by at most 1 ms (373),
  4 ms (1,224), 8 ms (2,481), 16 ms (1,757) or more (2; maximum 27.8 ms);
  3,489 frames were published, 2,830 within 40 ms of the previous one, 448
  within 70 ms and 207 later (idle stretches included; maximum 4,021 ms).
- These are counts from one boot of one run. They do not measure the 3D-off
  display stutter or the audio dropouts, and no threshold or criterion uses
  them. (Static reasoning.)

## Conditions

- The console was unlocked in all 69 samples. The sampler was started with a
  160 s limit by mistake and restarted at 16:40:52Z, so 16:34:49 to 16:40:52Z
  has no samples. BridgeVM Control was frontmost from 16:32:48 to about 17:06Z.
- The operator's browser had a window open behind Finder throughout; no input
  arrived for over 90 minutes before the run. No crash dialog appeared.

## Not verified

- A published receipt for this run; a second run.
- BitLocker unseal across a restart.
- Whether the record values change with the pointer, audio or display fixes;
  nothing here compares runs.
