# Post-merge integration, round 4: T19 r9, the HDA service tick and the pulled display export (2026-10-23)

Draft PR #428 (`agent/followups-r2-20261023`), continuing
[round 3](followups-r2-integration-r3-20261023.md). Evidence ranks follow
AGENTS.md: *live single run*, *automated*, *static reasoning*. Nothing here
promotes a criterion; A9 and A11 stay OPEN.

## 1. Seal `80733f99` checks (automated)

- Local full: PASS, all 45 stages, 22:51:21Z start, clean tree before and
  after (log SHA-256 `2997d811…`); shim suites 430 / 1465 (3 skipped) / 65 /
  454; only the seven expected A19 negative-test FAIL lines.
- Hosted full: run 40036162820 (job 120696044309) PASS, 44 stages,
  `checked_source_sha=80733f9959532ee3ceaacf0e59d91b513ce10cc8` (log
  SHA-256 `52f8023e…`), the same seven lines.
- All 103 workflow runs for `80733f99` succeeded.

## 2. T19 r9 at `80733f99`: PASS, BitLocker check refused by edition (live single run)

- Job `t19-80733f99-bitlocker-plumbing-pilot-r9`, tier
  `t19-windows-hvf-import-product-e2e`, ran 23:20:22–23:44:54Z on the T19
  source retained by T17 r91; `result=pass`, `exit_code=0`. Its input
  manifest (`db102193…`) carried the one opt-in row
  `bitlocker_restart_check<TAB>tpm-only`, and the preflight's verified
  inputs named it.
- 16 of 17 lane booleans true (`three_d_injection=False`, as in every T19);
  `failure_code=none`. `clipboard=True` is the first live run of the exact
  CLIPGET length and SHA-256-prefix check (round 3, section 3).
- TPM evidence: all three agent-ready boots `available`, TPM ready, PCR 0
  equal to the first boot.
- `bitlocker_restart_check` (private lane field): `outcome=refused`,
  `reason=edition-unsupported`. The enable boot reached READY and the guest
  workload reported `edition=home` with `pcr_read=ok`; the VM stopped by
  guest shutdown; the unlock boot was `not-reached`; restart, power-cycle
  and PCR-equality fields are explicit nulls. This is the expected stop on
  a Windows 11 Home source (round 3, section 4); it exercised the opt-in
  path end to end except the restart.
- Receipt: private and public SHA-256 `8618f7c3…`; strict verification
  with `--expected-commit` passes; `--require-claim-eligible` refuses
  ("installed-disk receipt alone cannot close A9"), as designed.
- Lane keys: "0 forgotten, 1 absent, 0 refusal(s)"; afterwards the keychain
  lists only `win11-typed`. Console unlocked in 178 of 178 samples; the
  window list was unchanged; no new crash report.
- Process note: the first submission (23:15:07Z) used a shell without
  `BRIDGEVM_LIVE_ROOT`, so it went to the CLI's default queue, which no
  worker drains. It never ran; its directory is kept with the private
  evidence, and the job was resubmitted at 23:20:16Z to the worker's queue.
- Confounders at submit: another tool's virtual machine at about 25% CPU,
  a worker compiling under `nice 15`, the operator's browser window.

## 3. HDA service tick and ring high-water mark (`cd0dcc0f`, `d7fcd2ab`, `cff790f7`, `9ad276f5`)

- From `agent/hda-service-tick-20261023`; see
  [the HDA record](hda-service-tick-20261023.md). The first commit
  conflicted only in appended budget rows, the last only in `@include`
  lines.
- `9ad276f5` replaces the deviation entry's recalled QEMU description with
  what QEMU master's `hw/audio/hda-codec.c` and `hw/audio/intel-hda.c`
  show: a 1 ms timer on `QEMU_CLOCK_VIRTUAL` moves output DMA, the default
  since 2018 (`bc753dc09f`) and the only mode since 2025 (`c2c8805742`).
- Accepted decision: bytes the 70 ms mark trims count as ring-full drops,
  so A5's `drops==0` stays at least as strict as before (the excess used to
  overflow the 500 ms ring into `ring_full_drops`). Any trim now fails A5
  and T17's audio stage: a raised bar, allowed by the rules.

## 4. Pulled display export, F1 (`62f1464e`, `02cd3fb5`, `a889d34a`, `67707beb`, `e2692807`)

- From `agent/display-export-20261023`; see
  [the display record](display-export-20261023.md). The first commit
  conflicted only in appended budget rows, the doc commit only in
  `@include` lines.
- A host bench (rank 3) put at most 18% of exporter wakes within 1 ms even
  at user-interactive QoS, so the export is pulled by the app's display
  tick (B2) instead of a faster timer (B1).

## 5. One shard for this PR's documents

- The 18 shards this PR added, one per document set, are folded into
  `post-merge-integration-docs-20261023.tsv` with this record's rows; every
  row is unchanged and main's shards are untouched. The root manifest drops
  from 136 to 119 lines. Its ceiling stays 136: the rows moved rather than
  disappeared, and the freed include lines are where later shards go
  (appending to a shard would raise that shard's ceiling, and no movable
  root row was left).

## 6. Integrated tree at `f02f5476` (automated)

- fmt; clippy for the workspace and with `venus`; probe example 727; HVF
  library 1493 (2 ignored); shim suites 430 / 1469 (3 skipped) / 65 / 454;
  the hand-listed Swift contracts outside `check-project.sh`; reset-record
  scan; Python, workflow, document and schema checks; selected-media, TPM
  PowerShell and HDA host-tail contracts; budgets at every commit with no
  raised ceiling; `git diff --check`. The 21 changed probe files gain no
  rustfmt hunk over `80733f99`.
- The first combined run failed two contracts that pin the exporter's stop
  record in `ramfb_display_thread.rs`:
  `ramfb-display-timing-host-tail-contract.py` (also inside the product E2E
  smoke) and `hvf-display-export-stop-order-contract.py`, which also named the old
  `stop.wait_until` loop. The display branch had moved that record, byte
  for byte, into `ramfb_display_loop.rs`, and its own checks did not run
  either contract. Both now read the new file and the new loop exit
  (`Wake::Stop => break`) with the same exact-count and order assertions.
  The fix is folded into `a889d34a`, the unpushed commit that moved the
  loop; both contracts pass at every commit of this round, and the product
  E2E smoke passes at the head.

## 7. What the next T17 first run must show

- HDA (from the HDA record): the poll-gap buckets over 20 ms and
  `capped_polls` at 0, `msis_sent` close to `ioc_completions`,
  `repeat_passes=0`, `ring_min_ms` above 0, `ring_max_ms` at most 70, and
  `ring_trimmed_ms=0`.
- Display (from the display record): a run.log line that the exporter
  answers frame requests, at least 50 ticks/s on a 60 Hz display,
  `late_le_1ms` at least 90% of ticks (r92: 6.4%), and `interval_le_20ms`
  as the largest interval bucket (r92: `interval_le_40ms`).

## Not verified

- The tick, the high-water mark and the pulled export have not run live;
  `hv_gic_send_msi` from a non-vCPU thread has not been tried on a guest.
- Nothing observed the screen; whether the window looks smoother needs the
  app-side counters and a timed window capture (screen recording is not
  granted).
- The BitLocker restart itself; it needs a Pro, Enterprise or Education
  guest.
