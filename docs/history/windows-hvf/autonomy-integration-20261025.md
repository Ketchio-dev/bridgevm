# Standing-autonomy integration: macOS 27 checks, platform-lock holds and an A9 release-claim verifier (2026-10-25)

Branch `agent/autonomy-integration-20261025` on main `430f3005`. Evidence
ranks follow AGENTS.md: *live single run*, *automated*, *static reasoning*.
Nothing here promotes a criterion: A9, A11 and A19 stay OPEN, and the product
state is unchanged.

## 1. Merges (automated)

- On 2026-10-25 the operator granted standing autonomy; AGENTS.md section 0
  records what agents may do without asking.
- #428 merged as main `a0618744`, tree-equal to seal `1b8772a1` (hosted full
  40060984259). All 53 post-merge workflows for `a0618744` succeeded.
- #429 changes only AGENTS.md and the pull-request template. Its head
  `7cacf314` passed all 45 local stages, hosted full 40149178377 and every
  required pull-request check. It merged as main `430f3005`.

## 2. The development Mac moved to macOS 27.0.1

- macOS 27.0.1 and Command Line Tools 27.0 were installed at
  2026-10-24T22:21Z. Xcode is 27.0 with Swift 6.4. Every earlier live run in
  this history ran on macOS 26.
- The local project check at `a0618744` passed all 45 stages in 19 minutes on
  that toolchain, with only the seven expected A19 negative-test FAIL lines
  (*automated*). Hosted CI runs on its own runners and is unaffected.
- Host observations, not product behaviour:
  - after the update reboot, the live-queue volume had to be reattached;
  - shells inside a launchd-started tmux server could no longer write to the
    external development disk, a TCC matter left to the operator;
  - signing with the development identity works only in a GUI session.

## 3. T17 on macOS 27 (live single runs, nonpromoting)

- **r96** `t17-a0618744-macos27-pilot-r96` failed with
  `input-selection-failed` at the ISO chooser's `selection-confirmation`.
  Location entry, sheet dismissal and the Open press had returned; the
  remaining 12.66 s of the 15 s chooser budget never showed the panel
  dismissed with the exact selected path. Cleanup was verified, and the
  receipt verifies strictly with `outcome=failed`.
- **Agent error:** r96's input manifest came from a superseded script, so it
  named symlinked input paths. Since r83, manifests copy the previous
  verified rows, which use resolved paths on the live volume.
  - *Static hypothesis, untested:* on macOS 27 the panel reports the resolved
    path, so the exact-path check never matches.
  - Load from Spotlight reindexing (about 22) is a second confounder.
- **r97** `t17-a0618744-macos27-staged-r97` used the r96 build and a staged
  manifest whose input paths are all resolved. It passed the three choosers
  and the offline Windows installation, and the VM was booting the installed
  disk.
  - Another operator session then took the console over Screen Sharing, so
    the agent cancelled the job at 01:47:00Z.
  - A deterministic contract runner in a worker lane had also launched a
    synthetic helper with `open -n` at 01:39:43–01:40:36Z; that launch
    failed with -10810.
  - r97 is not a sample and not a pass.
- **The cancel left processes running.** `bridgevm-live cancel` stopped the
  tier but left the product helper, app, runner and swtpm alive. The tier
  therefore reported `cleanup-failed`, and the worker fenced the queue.
  - The agent finished cleanup by hand: it copied the VM run log into the
    job's private directory, terminated the helper and app (the runner and
    swtpm exited with them), checked that no process or mount named the lane,
    removed the lane with `t17_owned_tree_cleanup.py`, removed the sealed
    worktree, moved the job to `done/` and cleared the fence.
  - It forgot the lane's vTPM key through the product's `export` and
    `forget-import` with exact proof. The Keychain again holds only the
    canonical guest's key.
  - The receipt verifies strictly with `outcome=cleanup-failed`.
  - A cancel path that leaves live processes and fences the queue is a
    live-tooling defect that stays open.
- No T17 run on macOS 27 has completed the journey.

## 4. Platform-lock holds record (automated)

- T17 r94 found the platform lock held on 835 of 1199 HDA service-tick
  attempts and had five poll gaps over 100 ms, but nothing named the holder.
- Every platform-lock site that can run beside the tick now takes the lock as
  a charged hold. Each busy tick is charged to the hold it found: `prerun`,
  `nvme`, `tpm`, `mmio`, `automation` or `host`.
- As the tick stops, it prints `platform lock holds:` with, per holder, the
  holds, the ticks they blocked, the holds over 20 ms and the longest hold.
  The record gates nothing and is not guest-visible.
- The probe example suite passes 736 tests (727 plus 9 new). Nine deliberate
  mutations each fail a new test.
- *Static reasoning:* NVMe doorbell exits, which run whole transfers and
  flush with `F_FULLFSYNC`, and the agent console's share writes, which also
  flush, are the likely long holders. No live run attributes r94's gaps yet.
  The record reaches the run log, which a failing lane keeps and a passing
  lane deletes. See the [platform-lock holds record](platform-lock-holds-20261025.md).

## 5. A9 release-claim builder and verifier (automated)

- `scripts/a9-release-claim.py` builds one combined A9 receipt from:
  - a T17 and a T19 release receipt at one commit, each accepted by its
    existing verifier;
  - exact-commit attestations of the hosted full check and of Security and
    quality, fetched by `scripts/fetch-a9-ci-attestation.py` and checked from
    the JSON alone;
  - a clean-host attestation bound by its SHA-256.
- `claim_eligible` holds only when no condition is unmet:
  - each journey is a release campaign passing 3/3;
  - both journeys are signed `developer-id-notarized`;
  - both CI runs concluded `success`;
  - the clean-host attestation is present and bound.
- It refuses mismatched commits, hosts or artifacts, pull-request Security
  runs and tampered inputs. It never edits the registry, and
  `criterion_pass` stays false.
- Neither existing journey rule changed. The contracts run inside the Windows
  product contract smoke; their self-tests and mutations fail when a rule is
  broken.
- A rehearsal with the retained T17 r91 and T19 r8 pilot receipts and live CI
  for `5de1f8ca` produced a consistent receipt with seven unmet conditions,
  so it is not eligible.

## 6. Still open

- A9 needs operator decisions on what counts as a clean machine and on
  Developer ID signing, then T17 and T19 release campaigns with an idle
  console. App-side display pacing records are in progress on a separate
  lane.
- The audio stutter needs a run log that keeps the new record from a run with
  capped poll gaps.
- The T17 cancel path must end the product processes it started.
