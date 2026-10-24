# Post-merge integration, round 6: T17 r95 passes at seal 43007ca0 (2026-10-24)

Draft PR #428 (`agent/followups-r2-20261023`), continuing
[round 5](followups-r2-integration-r5-20261024.md). Evidence ranks follow
AGENTS.md: *live single run*, *automated*, *static reasoning*. Nothing here
promotes a criterion; A5 stays PROVEN, A9 and A11 stay OPEN. This round adds
records only: the code is unchanged since `43007ca0`.

## 1. Seal `43007ca0` checks (automated)

- Local full: PASS, all 45 stages, 01:35:57Z start, clean tree (log SHA-256
  `8a02fdef…`); shim suites 430 / 1469 (3 skipped) / 65 / 454; only the
  seven expected A19 negative-test FAIL lines.
- Hosted full: run 40050526254 (job 120741956625) PASS, 44 stages,
  `checked_source_sha=43007ca04ac022de775b71857e66b1048c86c5fb` (log
  SHA-256 `f6260699…`), the same seven lines.
- All 103 workflow runs for `43007ca0` succeeded.

## 2. T17 r95 at `43007ca0`: PASS (live single run)

- Job `t17-43007ca0-audio-withdrawal-confirm-r95`, tier
  `t17-windows-hvf-product-e2e`, ran 01:58:41–02:37:03Z; `result=pass`,
  `failure_code=none`. Input manifest `9e80ec52…`; the app was built and
  development-signed at `43007ca0`.
- Every lane stage passed: 17 of 18 booleans, with `three_d_injection=false`
  as in every 3D-off lane. The exact CLIPGET check passed live for the third
  time (T19 r9, T17 r94, r95).
- Audio: the stage accepts the first run's last `hda CoreAudio stats:`
  record only with `frames_rendered>0` and `drops==0`, every total
  reconciled (`T17AudioCounters`); it passed.
- First-run host records (`first_run_host_records`, no guest data):
  - `hda poll:` 23880 polls, max gap 36 ms: 159 gaps of 10–20 ms, 10 of
    20–50 ms, none longer, `capped_polls=0`; MSIs 414 for 458 IOCs (90%;
    r94's journey session 73%, r92 65.5%);
  - `hda repeat:` 1 of 225 compared passes matched its previous lap (r92:
    30 of 277; r94: 31 of 264);
  - `hda stream:` 2 runs, 4597 ms running, a 20 ms cyclic buffer of two
    10 ms entries; the guest used neither the position buffer nor LPIB;
  - `hda CoreAudio timing:` 17339 callbacks, longest interval 11 ms, no
    late callback, no try-lock miss; the ring held 23–58 ms (final 53 ms)
    over 450 samples (r92: 0–114 ms);
  - `ramfb display timing:` 12693 ticks, 12499 (98.5%) woke within 1 ms;
    4638 frames published; intervals ≤20 ms 2126, ≤40 ms 1854, ≤70 ms 519,
    >70 ms 138, so `interval_le_20ms` was again the largest bucket.
- TPM evidence: available, present and ready at all three boots; PCR0 and
  PCR7 each had one value across them; boot 1 was read after a guest
  restart.
- Receipt `6ab5787e…` (private and public identical), strictly valid; the
  claim-eligible check refuses it, as for every pilot.
  `worker_cleanup_verified=true`; the lane key was forgotten (the keychain
  lists only `win11-typed`); the console was unlocked in all 230 samples;
  no new crash report. Its T19 source (18 GiB) is retained with a verified
  handoff.

## 3. Reading

- r95 is the first live run of the withdrawal. Its first run had no capped
  poll gap and delivered audio with `drops==0`, so A5's statement held in
  this run. It is one sample: r94 had five capped gaps in one session and
  r95 none, so whether capped gaps still occur under the tick with journey
  load is not decided (*live single runs*).
- The pulled display export met its live criteria for the second time.
- The tick's `max_busy_run_ms` was not observed: a passing lane keeps no VM
  run log, so the platform-lock holder during r94's gaps stays unknown.

## 4. Host housekeeping

- Under the approved deletion of old T19 sources, r91's retained source was
  removed with `retained_windows_cleanup.clear_directory` (no-follow
  descriptors); live free space went from 83.7 to 102.1 GiB, above the
  100 GiB admission floor. Only r95's source remains.

## 5. This round's commit

- The records commit adds this record, the archived `43007ca0` A11 note
  ([archive](../../archive/a11-measured-20261024-r95.md)) and a new
  manifest shard, with their budget rows; the next seal replaces the note.

## Not verified

- A capped poll gap since the withdrawal: r95 had none, so the path the A5
  capped-gap test pins has not run live.
- The latency hold after a capped gap (r92: 104 ms).
- The measurements that would decide audio 2 (lock contention), F3 (pointer
  pacing) and F4 (main-thread copy cost).
- Nothing observed the screen or listened to the audio.
- The BitLocker restart itself (Windows 11 Home source).
