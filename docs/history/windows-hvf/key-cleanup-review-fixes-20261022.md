# Key-cleanup review fixes and T19 r6 — 2026-10-22

Classification: robustness fixes for findings of an independent read-only
review of PRs #424 and #425 on 2026-10-22, plus two live single runs (T19 r6
and r7, nonpromoting). The fixes are automated-test evidence. A9, A11 and A19 remain
OPEN, and the product remains ENGINEERING_PREVIEW.

The review found no way for either PR to make a gate pass falsely. It did find
places where cleanup could report success while an imported or lane vTPM key
stayed in the login keychain.

## T19 key cleanup needs the product's proof (`a664c009`)

- **Finding (static reasoning):**
  - The product installs an imported key before it writes `vm.json`. The
    helper stops the app with SIGTERM, which skips the import's rollback.
  - So an import interrupted in between leaves a key under a directory with
    no `vm.json`, or under the requested slug with nothing published.
  - The cleanup looked only at published registrations and reported
    success. It also counted any exit 0 as proof and waited without a
    bound, so a Keychain prompt would have hung the lane.
- **Fix:** cleanup accounts for the requested slug and every real directory
  in the lane library, published or not; the install source cache is not a
  VM. Each `forget-import` counts in only two cases:
  - exit 0 with exactly `forgot_imported_key=<slug>`;
  - exit 1 with the product's exact missing-key refusal.

  Output is bounded to 64 KiB, and a call still running after 120 s is
  terminated and leaves cleanup unverified.
- **Tests:** a stub app emulates the product over a file keychain. Five of
  six new tests failed with the old cleanup; the sixth pins the missing-key
  line to the product source. One old test encoded the defect (cleanup runs
  only for a published import) and now asserts that the requested slug is
  always accounted for. ProductE2E shim suite: 403/0.

## Imported media mode set inside the workspace (`0d859958`)

- **Finding:** `a8ebc676` changed the imported disk and vars to 0600 through
  the library path after the move. `setAttributes` follows links, so a link
  swapped into that path could redirect the change. This is a same-user
  race, not a privilege boundary.
- **Fix:** each verified member becomes 0600 inside the import's private
  0700 workspace, and the rename carries the mode into the library.
- **Tests:** a recording `FileManager` showed both members moved at the
  selection's 0444, with mode changes under the bundle path; both checks now
  hold. HvfMediaImport tests: 9/0 with the real native helper.

## T17 lane-key forget follow-ups (`ae381000`)

- **Findings:**
  - The tool stopped at the first lane whose forget refused, so later lanes
    were never attempted.
  - Every forget left the exported recovery package and code in the lane.
    After a refusal, the kept tree therefore held material for a key that
    still exists.
  - An `OSError` printed a private path into the tier log.
- **Fix:**
  - Every lane is checked before any app runs, then each valid lane is
    tried, and each refusal is named.
  - A `finally` removes the package, the code and any scratch state. A
    signal that kills the tool can still leave them.
  - Errors print only their class and errno name.
  - A rerun is not idempotent: it refuses a lane already forgotten whose
    state holds data.
- **Tests:**
  - Existing suite: 4 of 14 failed before, 14/14 after.
  - New failure suite: `failures=11` across 8 tests before, 8/8 after.
  - T17 live-tier smoke: failed before with "the kept tree holds recovery
    material for a key that still exists"; now passes 51 checks.

## T19 tier accounts for imported keys (`9093e10e`, `97a3d838`, `cfd5a40f`)

- **Finding (static reasoning):** T19's `worker_cleanup_verified` meant only
  that the work tree was removed. A helper that crashed, hung or was killed,
  or whose own cleanup failed, left the tier reporting verified cleanup while
  an imported key could remain. T19 jobs are not fenced.
- **Fix:** the tier's `cleanup_work` runs `t19_lane_key_forget.py` after the
  process-absence check and before tree removal, on every cleanup path.
  - For each lane it checks the request's exact fields and paths and
    re-hashes the lane's sealed vTPM inputs.
  - It uses the candidate set and the proof contract above.
  - It tries every candidate, makes no call after a timeout or a changed
    executable, and prints no private path.
  - Any refusal makes cleanup fail, so the receipt says `cleanup-failed`
    with `worker_cleanup_verified` false and the tree kept.
  - A request-less lane may hold only the `inputs/` that the tier stages
    first.
  - The tier's inline `json_value` became the shared module. Over 50 T19
    calls, old and new gave identical output and exit status. The tier's
    ceiling fell from 69 to 64.
- **Tests:** a stub app over a file keychain.
  - Unit suite: 15 of 15 failed before the tool existed; 15/15 pass now.
  - Tier cases: before, 0 of 6 passed. Keys left by a helper, a crashed
    helper or a cancel between lanes, and a forget the product refused, all
    still reported verified cleanup. Now 6/6 pass, and the earlier pass
    fixture and adversaries still pass.
  - Staging cases: both were refused before `cfd5a40f`; 4/4 pass now.
- **Changed expectation:** a helper that rewrites its request now yields
  `cleanup-failed` with the tree kept, instead of `integration-failed` with
  cleanup verified, because the tier cannot account for a lane it cannot
  trust.

## T19 r6: the proof contract live

T19 r6 (`t19-a664c009-key-proof-pilot-r6`) ran app build r89 at `a664c009`
from 16:12:09Z to 16:41:36Z. Its input manifest SHA-256 begins `7fa1c768`.

- **Result:** pass, failure code `none`. All fourteen stages passed, cleanup
  was verified, and 3D injection was not used.
- **Receipts:** identical, SHA-256 beginning `f0749317`. The strict import
  verifier accepts both copies for `a664c009`, and with
  `--require-claim-eligible` refuses: an installed-disk receipt alone cannot
  close A9.
- **Console:** unlocked in all 61 samples, with no screen sharing and no
  fence.
- **Key (host observation):** afterwards the login keychain held 16
  BridgeVM vTPM items and no `bridgevm-a9-import-*` item. The real product's
  proof line therefore satisfied the exact-proof check. The missing-key path
  was not exercised live.
- **Probe changes:** the folder-share stage passed with the share-name limits
  in place. Whether a chunked PUT ran was not observed.
- **Limits:** this is a second nonpromoting pilot on the development Mac,
  from the same T17 r84 source. It is not a clean-machine receipt.

## T19 r7: the tier's key accounting live

T19 r7 (`t19-cfd5a40f-tier-key-pilot-r7`) ran app build r90 at `cfd5a40f`
from 17:00:54Z to 17:28:29Z. Its input manifest SHA-256 begins `2d2e301d`.

- **Result:** pass, failure code `none`. All fourteen stages passed, and both
  the lane's and the worker's cleanup were verified.
- **Receipts:** identical, SHA-256 beginning `20c1db4f`. The strict import
  verifier accepts both copies for `cfd5a40f`, and with
  `--require-claim-eligible` refuses, as it should.
- **Tier accounting (host log):** the job log reads "T19 lane vTPM keys: 0
  forgotten, 1 absent, 0 refusal(s)". The helper had already forgotten the
  key, so the real product answered the tier with its exact missing-key
  refusal. This is the path r6 did not reach.
- **Key (host observation):** afterwards there were 16 BridgeVM vTPM items
  and no `bridgevm-a9-import-*` item.
- **Console:** unlocked in all 58 samples, with no screen sharing and no
  fence.
- **Limits:** a third nonpromoting pilot from the same source, with the
  media mode set inside the workspace. It is not a clean-machine receipt.

## Not changed

- **Narrow product-side gap (static reasoning):** the product's missing-key
  refusal can mean "missing in the fallback keychain store" when its
  data-protection probe fails. Naming the store in that refusal would close
  this.
- **Wasted wait:** the import navigation still waits the full 1,200 s after
  the import shows an error.
