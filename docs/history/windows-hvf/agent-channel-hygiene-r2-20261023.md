# Agent-channel hygiene, round 2 — 2026-10-23

Classification: the two items that
[the first hygiene round](agent-channel-and-live-gate-hygiene-20261023.md)
left open, its harness-mode GET item and the rest of its item 1. Each fix was
written test-first and has its own commit. Each claim about behaviour below is
marked with its evidence: an automated test or static reasoning. **No live run
has exercised either change**, and no criterion or product state changes.

## 1. Harness-mode GETEND (`213f8291`)

- **Finding:** `finish_get` wrote
  `$BRIDGEVM_VIRTIO_CONSOLE_GET_DIR/<last component of the GETBEG path>` with
  `std::fs::write`.
  - That call follows a symlink at the name and writes into an existing file
    in place, so a hard-linked file elsewhere changed too. (Automated test.)
  - `handle_reply_line` accepted GETEND for whatever command was in flight.
    A GETEND with no GETBEG completed a GET, a RUN or a service control
    command. (Automated test.) That command's real reply then answers the
    next one. (Static reasoning.)
  - The completion rule covers every scripted and control-file GET,
    including the product's resident service, which tails a control file.
    Only the file write needs `BRIDGEVM_VIRTIO_CONSOLE_GET_DIR`, which the
    product clears. (Static reasoning.)
- **Fix:**
  - GETEND completes the in-flight command only when it is a raw
    `GET <b64(path)>` whose path the current GETBEG named. Any other GETEND
    is ignored like a stray line, and the command stays in flight.
    (Automated test.)
  - The argument is decoded with whitespace skipped, as the agent's .NET
    `FromBase64String` does, so a doubled space in a command still matches.
    (Automated test of the host side; the .NET rule is from its
    documentation.)
  - A reply that completes a command drops a GET reassembly still in
    progress, so a GETBEG followed by ERR cannot complete a later GET.
    (Automated test.)
  - The file goes through the share's `Destination`: opened relative to the
    GET directory with `O_NOFOLLOW`, written to a temporary file and renamed
    over the name.
    - A leaf symlink is refused and left in place; the GET line reports
      `write-error=Too many levels of symbolic links (os error 62)`.
      (Automated test.)
    - An existing regular file is replaced, never written in place, and no
      temporary file is left. (Automated test.)
    - A new file is created 0600, where `std::fs::write` gave 0644 under
      umask 022. (Static reasoning.)
  - The GETEND decision and `finish_get` moved to `share/harness_get_end.rs`,
    which can reach `Destination`.
- **Tests:** `get_end_tests.rs` adds 5.
  - On the unchanged source all 5 failed. A symlink's target and a
    hard-linked file outside the GET directory were overwritten. Stray
    GETENDs completed a GET, a RUN and a service control command. A failed
    GET's reassembly outlived the ERR that completed its command.
  - After the fix the probe example suite passes 657 (652 + 5).
- **Consequence:** a GET whose GETBEG names another path now stays in
  flight. A service control command is never retransmitted, so it waits for
  the agent's next READY. Scripted mode has no reply timeout and no READY
  resynchronisation, so it waits until the run ends. The agent echoes the
  decoded request path, so this takes a misbehaving guest or, possibly, a
  request path that is not valid UTF-8. (Static reasoning.)
- **Not changed:** the share sync's own GETEND (`handle_share_get_end`)
  still takes whatever reassembly exists. An ERR after a share GETBEG leaves
  that reassembly until the next GETBEG replaces it. A short GET is still
  written, reported `ok=false`. (Static reasoning.)

## 2. The T19 handoff's copy and refusal text (`df4d1290`)

- **Finding:** `scripts/live-gates/retain-windows-import-source.py` copied
  the vTPM state with `/bin/cp`, stderr inherited and `check=True`.
  - A failed copy put cp's error, naming the state path, in the tier log.
    `main` then printed the `CalledProcessError`, whose argv names the state
    and staging paths. (Automated test.)
  - `main` printed an `OSError`'s text, which names its path, and any
    `SubprocessError`'s text. (Automated test.)
  - The selected-media helper's 1800 s timeout raises such an error, naming
    the helper's argv. (Static reasoning; the test injects that error.)
- **Fix:**
  - `retained_windows_quiet.run_quietly` runs a child with stdout and stderr
    captured and never echoed. It is the runner the vTPM export already
    used, moved out of `retained_windows_vtpm.py` so both children share it.
    Output over 64 KiB, a run past 120 s, an exec failure and a nonzero exit
    are each refused with fixed text. The text names only the errno or the
    exit status. (Automated test: the export cases cover all four through
    the export, the new cases the copy's exit status.)
  - `retained_windows_vtpm.copy_vtpm_state` creates the private directory
    and runs the copy through it. The cp argv is unchanged.
  - `main` prints a `ValueError` or `KeyError` refusal's own text as before.
    An `OSError` prints only as `operating-system error (<errno name>)`, and
    a `SubprocessError` only by its class name. (Automated test.)
- **Tests:** `retained_windows_refusal_cases.py` adds 3. They reuse the
  export cases' fixture and run `main()` with descriptors 1 and 2 captured,
  inside `tests/integration/retained-windows-publication-contract.py`.
  - On the unchanged tool all 3 failed. cp's `Permission denied` line and
    the cp argv were printed with both paths, an `EEXIST` printed the staging
    path, and a helper timeout printed its argv.
  - After the fix the publication contract passes 13 of 13 (10 + 3).
  - The selected retention (4) and mutation (2) contracts,
    `scripts/check-retained-windows-contracts.sh` and the Windows product
    E2E live-tier fixture smoke pass. The smoke's 51 checks include a handoff
    through the real copy.
- **Bound:** 64 KiB is a refusal threshold, not a memory bound.
  `subprocess.run` reads all of a child's output before its length is
  checked, as for the export since `1cfcdaad`. cp writes about one error
  line per failing entry, and the tool accepts at most 1,024 state entries.
  (Static reasoning.)
- **Not changed:** `ValueError` and `KeyError` texts from the tool and the
  modules it loads are still printed. This round did not audit each one for
  paths.

## Checks and budgets

- Rust: `cargo fmt --all --check`, and clippy with `-D warnings` over the
  workspace and over `bridgevm-hvf` with `venus`, pass.
  - `rustfmt --check` passes for `harness_protocol.rs` and the new files.
    For `share.rs` it reports only the unchanged hunks of its other child
    files.
  - The required clippy runs do not lint the probe example's test-only
    code. Linting it in test mode as well reports three findings, all in
    files these commits did not change.
- Python: `scripts/check-python-scripts.py`.
- Budgets lowered after the moves: `harness_protocol.rs` 521 to 507 and
  `retained_windows_vtpm.py` 31 to 22. The four new code and test files,
  this record and its manifest shard are registered at their measured sizes.
  Each commit's budgets were checked on its own tree.

## Not verified

- No live run, real T19 tier or guest agent has exercised these changes.
- `scripts/render-capability-status.py --check` is expected to fail on
  these unsealed commits; it is not claimed to pass.
