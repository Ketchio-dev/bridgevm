# Post-merge integration, round 5: T17 r94 fails at audio and the ring high-water mark is withdrawn (2026-10-24)

Draft PR #428 (`agent/followups-r2-20261023`), continuing
[round 4](followups-r2-integration-r4-20261023.md). Evidence ranks follow
AGENTS.md: *live single run*, *automated*, *static reasoning*. Nothing here
promotes a criterion; A5 stays PROVEN, A9 and A11 stay OPEN.

## 1. Seal `79da0472` checks (automated)

- Local full: PASS, all 45 stages, 00:04:49Z start, clean tree (log SHA-256
  `47c87df6…`); shim suites 430 / 1469 (3 skipped) / 65 / 454; only the
  seven expected A19 negative-test FAIL lines.
- Hosted full: run 40043167583 (job 120718702367) PASS, 44 stages,
  `checked_source_sha=79da0472872665d919ebe4d49c6d7e41d8adfcee` (log
  SHA-256 `ade0fe87…`), the same seven lines.
- All 103 workflow runs for `79da0472` succeeded.

## 2. T17 r94 at `79da0472`: FAIL at the audio stage (live single run)

- Job `t17-79da0472-audio-display-confirm-r94`, tier
  `t17-windows-hvf-product-e2e`, ran 00:27:36–00:47:39Z; `result=fail`,
  `failure_code=guest-evidence-missing`: "guest audio lacked successful
  host CoreAudio counters". Input manifest `0217b084…`.
- Passed before it: install, Secure Boot provisioning, first READY,
  keyboard and pointer, clipboard (the exact CLIPGET check, live for the
  second time), folder share and network. Not reached: snapshot, second
  READY and shutdowns.
- The journey's first-ready session (host records from the kept
  diagnostic run log, no guest data):
  - `hda service tick: stopped streams=2 ticks=1199 busy=835 msis=100`:
    835 of 1199 tick attempts (70%) found the platform lock busy;
  - `hda poll:` 33 gaps of 20–50 ms, 2 of 50–100 ms, 5 over 100 ms
    (max 394 ms), `capped_polls=5`, `capped_lost_ms=631`; MSIs 393 for
    536 IOCs; `repeat_passes=31` of 264 compared;
  - `hda CoreAudio timing:` `ring_max_ms=70`, `ring_trimmed_ms=178`;
  - `hda CoreAudio stats:` `frames_rendered=249264 drops=275
    ring_full_drops=275`, which the T17 audio check refuses;
  - the display exporter in the same session woke once 119 ms late and
    had one 64 ms tick: the host also stalled other threads.
- An earlier audio session of the same run (18.6 s of stream) had
  `busy=521` of 3716 attempts, one capped gap (231 ms), MSIs 1784 for
  1845 IOCs and 48 ms trimmed (`drops=289`).
- Display (pulled export): every session printed "answering frame
  requests"; 98–99% of export wakes came within 1 ms of the app's request
  (r92: 6.4% within 1 ms of the deadline), and `interval_le_20ms` was the
  largest interval bucket in both journey sessions, as the display record
  required. Nothing observed the screen.
- Receipt `ee306810…` (private and public identical), strictly valid,
  `outcome=failed`; the lane key was forgotten (keychain lists only
  `win11-typed`); console unlocked in 121 of 121 samples; no T19 source
  retained; no new crash report.

## 3. Reading and decision

- A5 says "Guest PCM reaches the host CoreAudio ring with frames_rendered>0
  AND drops==0". Bytes the 70 ms mark trims never reach the ring, so they
  are drops, and with capped gaps still occurring (*live single run*) the
  mark breaks a PROVEN statement in ordinary use. The trimming is
  withdrawn; counting trims elsewhere would have relaxed A5.
- The tick stays: MSIs per IOC rose from r92's 65.5% to 96.7% and 73%.
  It cannot poll while another thread holds the platform lock, and why the
  lock stayed busy for hundreds of milliseconds during the journey's audio
  stage is unknown (*totals only*); the tick's stop line now reports
  `max_busy_run_ms` to attribute the next gaps.

## 4. Withdrawal (`99c458b9`, `ef3b8a25`, `1e5322c1`)

- From `agent/hda-mark-withdrawal-20261024`, written on `79da0472`; see
  [the withdrawal record](hda-mark-withdrawal-20261024.md). Its red test
  commit and the withdrawal are folded into `99c458b9` so every commit
  stays green; the red result is in that commit's message.
- `99c458b9`: a new test pins A5 in a capped gap (every polled byte reaches
  the ring, all drop counters 0); it fails against the trimming source and
  passes once the producer appends every complete frame again. The
  `hda CoreAudio timing:` record is back to its 14 fields in every grammar;
  four tests of the withdrawn trimming are removed with their files.
- `ef3b8a25`: the tick's stop line ends with `max_busy_run_ms`, the
  longest run of consecutive busy-skipped ticks; no reader parses that line.
- `1e5322c1`: the withdrawal record and its shard.
- The latency hold after a capped gap (r92: 104 ms) is open again; closing
  it without dropping PCM needs a sample-rate converter or a re-prime that
  keeps every byte.

## 5. Integrated tree (automated)

- On the tree of `1e5322c1` (checked as `4bfc4ae4`, the same tree before
  the doc edit moved): fmt; clippy for the workspace and with `venus`;
  probe example 727 (4 tests removed, 4 added); HVF library 1493 (2
  ignored); shim suites 430 / 1469 (3 skipped) / 65 / 454; the hand-listed
  Swift contracts; reset-record scan; Python, workflow, document and schema
  checks; product E2E smoke; selected-media, TPM PowerShell and host-tail
  contracts; `git diff --check`. Budgets and both document checks pass at
  each of the three commits; no ceiling rose; the probe files gain no
  rustfmt hunk over `79da0472`.
- The branch's own first run of the product E2E smoke, made while its shim
  and cargo tests ran in the same tree, failed once ("private index identity
  or schema differs"); run alone it passed there, in a scratch worktree and
  at `79da0472`, and it passed in the sequential run above. Its cause is not
  known.

## Not verified

- No live run at the head of this round yet; the next T17 must show
  `drops=0` in its first-run stats and report `max_busy_run_ms`.
- The platform-lock holder during the long gaps.
- The BitLocker restart itself (Windows 11 Home source).
