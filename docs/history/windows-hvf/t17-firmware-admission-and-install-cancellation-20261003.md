# T17 failed pilot, firmware admission and install cancellation — 2026-10-03

## Frozen live failure

**Evidence rank: live single failed run.** Physical job
`codex-t17-8e64bd43-bounded-observation-pilot-r1` used sealed source
`8e64bd43355585526224b3b79cce8d7610c2af95`, a development-signed app and 3D disabled.
The strict [public failed receipt](../../windows-arm/evidence/t17-8e64bd43-bounded-pilot-failure-20261003.json)
is copied byte-for-byte: 2,661 bytes, SHA-256
`5cc2720e60cd82259d2b41172b89bd1193c901ce99053588c597743859387083`.
It finished at 07:07:17 UTC with one attempt, zero passes, one failure and
`outcome=failure_code=cleanup-failed`. All 15 public stage counts are zero;
`worker_cleanup_verified`, `pass`, `claim_eligible`, `criterion_pass` and promotion are false.
No lane authentication stamp was present. The raw lane's installer timeout and
artifact/VM-created/app-cleanup flags are unauthenticated and do not override these counts.
No T19 source retention or live import sample was established.

The helper's Foundation `Process` handle reported app-only cleanup; that did not
prove installer teardown. Native PID 60840 and its parent 60824 were still alive
with retained identity at 07:15:11 UTC. Root's manual cleanup attempt failed its
command guard (exit 1), SHA-256
`ff15253f62e147f2ccb55a231626882761afefc9fb0c2722320124da8c525d5a`.
Signal delivery was not durably recorded and is unclaimed. A subsequent query
found both PIDs absent at 07:19:04 UTC; this is not proof of all job residue gone.
The failed receipt and cleanup fence remain failure evidence.

## What the host diagnostic establishes

**Evidence rank: actual host sample plus static source/binary interpretation.**
The native log stayed at 341 bytes, ending after `hv_gic_create`; no framebuffer
capture or guest-progress state was available. Its SHA-256 is
`653b2376d7aaa4069c0f4cc729d187b2739654554610788f1b046f1e459744f6`.
A one-second sample counted 85 main-thread samples in `File::open`/`open`.
Probe SHA-256 `9c8eeab5c46f159623eca4c6da61cbb44752c2ed0524d8f0de43da9154c56656`
and UUID `493377A7-2075-33B6-8061-CFFB219EF6E3` bind the sampled callsite to
firmware-code loading before guest RAM/vCPU initialization and watchdog arming.
The actual CODE environment override was absent. A compiled build-path dependency
is a source inference; the actual opened pathname and blocking cause were not captured.
Computer-use safety refused the protected NotificationCenter read; no bypass occurred.
No Windows or OS-notification cause is established.

**Retraction:** the earlier whole-installation “25-minute bound” wording was wrong.
The 1,500,000 ms boot watchdog starts after native host startup; the helper's
1,800-second terminal-UI wait is separate. Both durations remain unchanged.
Neither proves a whole-pipeline wall deadline or descendant teardown.

## Deterministic source repairs and preserved failures

**Evidence rank: automated tests and static review, with no new live result.**
Integrated cancellation source `0961533febd32c01ef504fadba593a690b44c50c`
sets its install-attempt flag before pressing Start. Cleanup selects the owning
install-cancel control and waits for a terminal install acknowledgment within
the existing 30-second wait; missing/error/nonterminal observations remain refused
despite later app exit. Runtime `SYSTEM_OFF` observation and app 10/5-second
fallback waits remain separate. Outer process/mount/residue fences are unchanged.
The owned shell/child with injected UI baseline failed three of four cases with
16 assertions (log SHA-256 `ce01ea16a1b0b46305bf9a286cd75cb8d4ffc3a36657dcdaab85262aff5cb29e`).
Final affected XCTest passed 287 product cases and 148 install/control cases,
435 total including five new cases; independent replay passed the same five.
Final log SHA-256: `9312e003c8d9b2334af46ee64fa049aac6d77d723d416af5c21de038c27106d9`.
These disposable host stand-ins do not prove Windows guest cancellation.

Integrated firmware source `03ac31f598016796dfd4d04ee293d3ae4092aa90`
selects and verifies pinned bundle firmware and passes CODE explicitly.
Missing, corrupted or symlinked inputs refuse before evidence/media creation;
packaged selection has no repository/build-tree fallback. Eleven Python cases,
two staging cases, the comparator contract and one owning Swift XCTest passed.
The relocated fake-probe baseline failed with CODE absent (log SHA-256
`b1329665cd688ff96126e801020bb84ab47f9368f010f487f7fe2e4b26688464`).
The initial Swift fixture failed its `/var` versus `/private/var` spelling assertion
(log SHA-256 `f0a22a10d903de81000c595026398ee04f0c7e6c8f0ab6ba788d0a9c7022cf06`);
the repaired fixture checks exact selected-file device/inode identity.
Both failed experiments stay retained; neither establishes the actual blocked pathname.

The preceding exact 8e64 source passed 44 local project steps and 43 applicable
hosted full-check steps in [run 37101107906](https://github.com/Ketchio-dev/bridgevm/actions/runs/37101107906).
Its [core CI run 37101108931](https://github.com/Ketchio-dev/bridgevm/actions/runs/37101108931)
passed 13 required jobs; optional CGL initialization still failed with error 10002
and exit 101 despite advisory success. Opt-in native skips remain explicit.
Those results neither erase this failed pilot nor validate the new combined source.
The frozen receipt's absent CI run fields and false CI flags remain byte-identical.
Integrated full local/exact-hosted checks and a new matching artifact/live result
remain pending. No criterion or product wording is promoted; A9/A11 remain OPEN.
