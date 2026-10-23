# Agent-channel and live-gate hygiene — 2026-10-23

Classification: four small fixes for findings from read-only reviews on
2026-10-22. Each fix was written test-first and has its own commit. The
evidence is automated tests and static reasoning. **No live run has exercised
any of these changes**, and no criterion or product state changes.

## 1. The T19 source handoff no longer echoes product output (`1cfcdaad`)

- **Finding:** `scripts/live-gates/retain-windows-import-source.py` ran the
  packaged app's `--vtpm-lifecycle export` with stdout discarded and stderr
  inherited, so a product refusal reached the tier log verbatim. `check=True`
  turned a nonzero exit into a `CalledProcessError`, and `main` printed its
  text: the full argv, including the private state, package and code paths.
  Live-gate logs may carry only booleans, sizes, hashes and fixed tokens.
- **Fix:** the export runs through `retained_windows_vtpm.export_vtpm`. That
  call captures stdout and stderr with a 64 KiB bound and a 120 s timeout, as
  `t17_lane_key_forget.py` does, and never echoes them. A nonzero exit, a
  timeout, oversized output or an exec failure each raises a fixed refusal
  that names only the exit status or errno. `retain()` still removes its
  staging and writes no status, and the tool exits 1. The argv is unchanged.
- **Tests:** `retained_windows_export_cases.py` runs `main()` the way the tier
  does, with descriptors 1 and 2 captured.
  - On the unchanged tool, 4 of its 5 cases failed: a refusal's stderr and
    argv were printed with paths, 65,537 bytes of stderr were echoed and the
    export was accepted, an unexecutable app printed its path, and a hung
    export had no bound.
  - After the fix, the publication contract passes 10 of 10. The retention and
    mutation contracts pass unchanged (4 and 2 tests).
- **Not changed:** the `/bin/cp` copy of the vTPM state still inherits stderr
  and uses `check=True`, and `main` still prints an `OSError`'s text. Either
  can name a private path. This remains a follow-up.

## 2. A stop signal no longer strands T17 recovery material (`9b019dc4`)

- **Finding:** `scripts/live-gates/t17_lane_key_forget.py` removes each lane's
  vTPM recovery package and code in a `finally` block. Python's default
  SIGTERM and SIGHUP actions end the process without running it, and a queue
  cancel sends SIGTERM to the tier's process group.
- **Fix:** `main` runs through `stop_signals.stoppable`, which turns SIGTERM,
  SIGINT and SIGHUP into `SystemExit(128 + signal)`.
  - The cleanup then runs, `subprocess.run` kills and reaps a blocked app, and
    the exit is nonzero, so the tier still reports `cleanup-failed`.
  - The first signal sets all three to be ignored, so a repeated signal cannot
    interrupt the cleanup.
  - The key stays, because the interrupted forget proved nothing.
- **Tests:** `t17_lane_key_forget_signal_cases.py` blocks the packaged app in
  `forget-import` after its export wrote both files, then signals the tool.
  - On the unchanged tool, SIGTERM exited -15 and SIGHUP -1, each leaving the
    package, the code and a live app.
  - After the fix, each signal exits 128 + its number, with no material and no
    app left. The failure test passes 10 of 10 (8 before), and the signal case
    passed 8 of 8 repeats.
  - The main forget test (14) and the live-tier smoke (51 checks) pass.

## 3. Refused guest share names are reported, bounded (`2efe027d`)

- **Finding:** since `f46e038f`, a guest listing name of 1024 bytes or more,
  or with a component over 255 UTF-16 units, has been dropped with a bare
  `continue`. A guest file that can never sync left no trace in `run.log`.
- **Fix:** `parse_ls_into` returns a `RefusedNames` value. It keeps the count
  of refused names, plus the fixed reason and byte length of the first three,
  and a fingerprint of them all; it stores no refused name.
  - Each listing prints at most three lines such as
    `BVAGENT SHARE skip <guest-name> path-too-long 5000`, plus one count line.
    It prints nothing while later listings refuse the same names.
  - The placeholder `<guest-name>` cannot be a Windows name, and the app's
    `run.log` parser ignores skip lines.
  - `..` names are still refused without a report. No GET is issued for any
    refused name.
- **Tests:** `share_listing_report_tests.rs` pins the report lines, the bound
  for a 10,000-name listing (4 lines, count 10000) and the once-until-changed
  rule.
  - Against the unchanged source it did not compile, because no report
    existed. With the noting disabled, all 3 tests fail.
  - The probe example suite passes 641 (638 + 3).

## 4. Guest GETs over the share cap are refused (`dbc93885`)

- **Finding:** the share cap (`BRIDGEVM_VIRTIO_CONSOLE_SHARE_MAX_KB`: 8192 KB
  in the scripts, 65536 KB in the product) was enforced guest-to-host only on
  the size a listing names. The GET that fetches that version returns
  whatever the file holds by then. `handle_share_get_end` wrote it after
  checking only the GET's own declared total, up to the 64 MiB reassembly cap.
- **Fix:** `ShareSync::on_guest_file` refuses a fetched file over the cap as
  `Skip(TooLarge)`, printed like an oversized listing:
  `BVAGENT SHARE skip <name> too-large <size>`.
  - Nothing is written, and an already-synced host copy stays as it was.
  - Listings that still name the refused version do not fetch it again. A new
    version is fetched as before.
  - In-cap files take the unchanged path. The GET is still reassembled up to
    64 MiB before it is refused.
- **Tests:** `share_get_cap_tests.rs` runs under a 1 KiB cap.
  - On the unchanged source, all 3 tests failed: a 1025-byte file was written,
    a synced 8-byte host copy became 4096 bytes, and the written file then
    drew a host delete.
  - After the fix, they pass, and a 1024-byte file at the cap is still
    written. The probe example suite passes 644 (641 + 3).

## Not done

- **Harness-mode GET (item 5):** `finish_get` in
  `agent_console/harness_protocol.rs` writes through `std::fs::write`, which
  follows a symlink at the leaf name, and GETEND completes whatever command is
  in flight. Harness mode is reachable only when the probe runs directly with
  `BRIDGEVM_VIRTIO_CONSOLE_GET_DIR` set; the product clears the environment.
  The worker reached its deadline with only red tests for this item. They are
  kept outside the repository with the other evidence, and the item stays a
  follow-up.

## Not verified

- No live run, real T17 or T19 tier, or guest listing has exercised these
  changes.
- The structural budgets were lowered or registered at measured sizes in each
  commit. The checks for each commit are listed in its message.
