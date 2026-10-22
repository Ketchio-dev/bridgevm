# T17 lane-key cleanup and agent-channel hardening — 2026-10-22

Classification: robustness fixes for defects found by read-only reviews on
2026-10-22. The evidence is automated tests and static reasoning. No live run
has exercised either change. A9, A11 and A19 remain OPEN, and the product
remains ENGINEERING_PREVIEW.

## T17 never forgot its lane vTPM key

- **Reading:** the product creates a login-keychain vTPM key, named by the VM
  slug, the first time a VM starts with empty vTPM state. The T17 helper only
  stops processes. The tier's cleanup then deleted the lane tree, vTPM state
  included, after a process-absence check. Deleting a VM in the product does
  not delete its key either.
- **Host observation:** the login keychain holds 15
  `bridgevm-t17-lane-1-*` keys from earlier pilots. 13 date from 2026-10-01
  and 2026-10-02, and the other 2 are T17 r83's and r84's. Those receipts
  reported `worker_cleanup_verified`.
- **Fix `111e05e2`:** cleanup runs `t17_lane_key_forget.py` after the
  process-absence check and before removing the tree. Passing, failing,
  cancelled and signalled runs therefore all forget their keys, and an
  optional T19 handoff export still runs first. For each lane the tool:
  - re-applies the request's identity and fixed-path rules, and refuses a
    lane library that holds another VM registration;
  - re-hashes the sealed app executable before each app call and gives each
    call 120 s;
  - exports the lane key into a new private folder inside the lane, then runs
    `--vtpm-lifecycle forget-import` with the same stable ID, state, package
    and code. It requires exit 0 and exactly `forgot_imported_key=<slug>`.
    Export and forget see the same stopped state, so the fingerprint check
    that refused T19 r4's cleanup holds here;
  - treats a lane with empty or absent state as keyless only when export
    reports the product's missing-key message.
  - Any other outcome exits 1. The tier then reports `cleanup-failed` and
    keeps the tree and the key.
- **Size:** the tier's `json_value` moved unchanged into
  `product_e2e_json_value.py`. The tier went from 121 to 112 lines, and its
  ceiling was lowered to match.
- **Tests:** these use a stub app over a file keychain, never a real app or
  Keychain.
  - The new `t17-lane-key-forget-test.py` failed before the tool existed
    (25 failures, 2 errors in its log) and now passes all 15 tests.
  - The T17 live-tier smoke first failed with "key-forget-pass-fixture
    reported verified cleanup but its lane key survived" and now passes 51
    checks.
- **Not changed:** the 15 existing keys stay. Removing them changes the
  operator's keychain and needs approval.

## Guest agent channel

The review found no host escape, path traversal or symlink following on the
paths a guest can drive to write, delete or read host files. It found four
low-severity defects a guest can trigger; two are fixed here.

| Commit | Defect and fix | Test before → after |
| --- | --- | --- |
| `17a83e4b` | A chunked host-to-guest PUT computed `seq + 1` on the guest's `OK PUTCHUNK <seq>`. Release builds keep overflow checks, so `seq = 2^64 - 1` panicked the probe's CPU0 thread while it held the platform mutex. `checked_add` now ignores that reply, like any sequence number that does not match. | overflow panic → pass |
| `f46e038f` | Guest share names had no length or depth limit, and the unseen-name check hashes every `/` prefix, which is quadratic. Keys of 1024 bytes or more, or with a component over 255 UTF-16 units, are now refused like `..` names. No macOS path can hold them. | a 200,000-component name ran past the 10 s bound and a 1024-byte key was accepted → both pass in 0.06 s |

- **Limits checked on this Mac (APFS):**
  - a 1023-byte path returns ENOENT, and a 1024-byte path ENAMETOOLONG;
  - a component of 255 Hangul syllables (765 bytes) is created, while 256
    syllables or 128 emoji are refused.
  - The NTFS limit comes from documentation.
- **Checks:** the probe example suite went from 627 to 630 passed. Clippy
  with `-D warnings`, rustfmt and the structural budgets pass.
- **Timing:** a release-mode copy of the unseen check, not the product path,
  took 6.8 s for one 200,000-component name. The worst name under the new
  limits costs about 56 µs.
- **Not changed, operator decision:** the 8 KiB `clip_max_bytes` limit is
  deliberately host-to-guest only, because the guest's reader is slow. The
  host reads a guest `CLIP` reply up to the 32 MiB line limit, about 24 MiB
  of text. A6 promises clipboard round trips in both directions with no size
  limit. Whether a guest may place that much on the Mac pasteboard is a
  product decision.
- **Not changed, plan only:** one 32 MiB `LSOK` listing line can queue
  millions of share GETs, because the 1024-entry queue cap covers
  control-file commands only. The plan:
  - refuse an over-limit listing whole, never truncated, because names
    missing from a listing delete host copies;
  - cap queued GETs per listing;
  - serve clipboard and control requests first;
  - track pending requests by count.
- **Information:** guest-controlled line breaks can forge app-visible
  `run.log` lines. This has the same root as the raw `GETBEG` path print.
