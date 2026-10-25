# Guest write policy follow-ups — 2026-10-23

Classification: the five findings the 2026-10-23 review left open on the
policy of `docs/history/windows-hvf/guest-write-policy-20261023.md` and
`docs/history/windows-hvf/guest-write-policy-revised-20261023.md`, closed
test-first on seal `8d3eb052` in `b4c0942b`, `45b44884`, `c6a4a3e8`,
`150c592b` and `6a2145e4`. Product decisions here were delegated by the
operator on 2026-10-23; the conservative choice was taken and is recorded.
**No live guest run has exercised any of this.** No criterion, registry entry
or product state changes here. Ranks are those of `AGENTS.md`: rank 3 is an
automated test, rank 4 static reasoning. Red and green logs are kept outside
the repository with the other evidence of this work.

## 1. The app's "Copy to Mac clipboard" route (`b4c0942b`)

The daemon stores a guest-tools `ClipboardChanged` text as
`GuestToolsRuntimeMetadata.clipboard` and serves that record unchanged in
`guest_tools_status`; the app decoded `last_clipboard`, so its Guest Clipboard
panel never had a text and its button was unreachable [rank 4].

- **Decision: fix the key, do not remove the route.** The button is a
  user-visible control in the always-shown guest-tools panel of the VM detail
  view. The app now reads `clipboard` only; no alias is kept for a key the
  daemon never wrote.
- **Same limit as the probe route.** Text over 8 MiB (8,388,608 bytes) of
  UTF-8 is refused whole, never truncated, and the Mac pasteboard keeps what
  it holds. The button is disabled for it and the preview shows only its
  size (`apps/macos/Sources/BridgeVMApp/Models/GuestClipboardCopyLimit.swift`).
  The JSON text arrives decoded, so there is no base64 length to check
  first; the guest-tools frame limit (1 MiB) already bounds it [rank 4].
- **Tests** [rank 3]: `apps/macos/Tests/BridgeVMAppTests/GuestClipboardCopyRouteTests.swift`.
  On the unchanged source 4 of 5 failed: the daemon's field was not decoded,
  a `last_clipboard` key was read, and 8 MiB + 1 byte and 8,388,609 bytes of
  Hangul were written; exactly 8 MiB is copied whole before and after
  (`item1-2-swift-red.log`, `item1-swift-green.log`). BridgeVMApp shim suite
  425 to 430. The wire-format fixture of `DaemonDTOTests.swift` named the
  field `last_clipboard`; it now uses the daemon's key with the same
  assertion. `crates/bridgevm-storage/src/tests_split/guest_tools_clipboard_key.rs`
  pins the producer: the record serializes to the exact JSON the Swift test
  decodes (`item1-storage-pin.log`).

## 2. Operator CLIPGET in run.log (`45b44884`)

A CLIPGET from the control file or a scripted command printed the decoded
guest text between `BVAGENT CLIP` and `BVAGENT END` lines, up to the 32 MiB
line limit. It now prints one bounded line, never the text:
`BVAGENT CLIP <command> bytes=<decoded bytes> sha256=<first 16 hex digits>`,
or `base64-error=<kind> payload-bytes=<n>` for a malformed payload
(`crates/bridgevm-hvf/examples/hvf_gic_boot_probe/agent_console/clip_operator_reply.rs`).
The automatic clipboard sync already printed byte counts only.

- **Readers changed with it.** The T17 journey's CLIPGET check
  (`apps/macos/Sources/BridgeVMProductE2E/T17ControlReply.swift`) and the A6
  and A4 scripts (`scripts/verify-agent-clipboard-share.sh`,
  `scripts/verify-keyboard-input.sh`) compare the length and SHA-256 prefix
  of the text they set. T17 used to accept a frame that *contained* its text;
  it now needs an exact match. A text the guest returns with CRLF line ends
  would now fail it [rank 4: the agent serves the text it was set from its
  cache].
- **Tests** [rank 3]: `clip_operator_reply_tests.rs` handles four replies in a
  child of the test binary and reads that child's stdout, since the harness
  captures println!. On the unchanged source all four stand-in markers
  reached stdout, 8,389,078 bytes (`item2-clipget-red.log`). Now four lines
  match prefixes computed with `shasum -a 256` outside the code. Probe suite
  693 to 694 (`item2-probe-green-full.log`). The two T17 tests in
  `T17ClipboardFingerprintReplyTests.swift` failed on the unchanged source;
  ProductE2E shim suite 436 to 438 (`item2-swift-green.log`). The CLIPGET
  fixture of `T17ControlReplyTests.swift` is the new line, since the probe no
  longer prints the old frame. A host check ran the scripts' own commands on
  their texts against a synthetic run.log (`item2-script-format-host-check.log`).
- The 16 hex digits identify a known text; a short, guessable guest text can
  be confirmed from them by trying candidates [rank 4].

## 3. Refused share writes are not fetched again (`c6a4a3e8`)

- **What persists.** The guest write policy now marks the refusals that hold
  until the guest file changes
  (`crates/bridgevm-hvf/examples/hvf_gic_boot_probe/agent_console/share/guest_write_refusal.rs`):
  execute bits the share keeps, a quarantine mark refused with ENOTSUP,
  EACCES or EPERM (a deny ACL, a flag), and an msdos/exfat mark that does not
  read back. Messages are unchanged. Everything else is retried at the next
  listing as before: no CSPRNG bytes, ENOSPC, EDQUOT, EIO, an unwritable share
  directory, a read-only destination.
- **The table.** Per guest path, the refused version's size and the FNV-1a
  hash of its listed mtime (the guest picks that string, so it is not kept).
  A listing naming the same version does not fetch it; a new size or mtime,
  or the file leaving the listing, forgets it. **Bound: 1024 entries, the
  oldest forgotten first**, so it is fetched once more. Memory only: a
  restarted probe fetches each file once more
  (`crates/bridgevm-hvf/examples/hvf_gic_boot_probe/share_write_failures.rs`).
- **Limitation.** A refusal caused on the host side (a deny ACL on the
  replaced file, a share without extended attributes) is not retried when the
  host side is fixed, only when the guest file changes, the entry is evicted
  or the probe restarts.
- **Tests** [rank 3]: `share_write_refusal_tests.rs` drives the service loop
  against a real inherited deny-writeextattr ACL. On the unchanged source the
  unchanged refused file was fetched 17 times in 36 ticks and all 1025
  refused files were fetched again (`item3-refusals-red.log`); now once, and
  only the evicted oldest. The transient-failure test passed before and
  after. `guest_write_refusal_tests.rs` pins the classification, with
  ENOTSUP, execute bits and msdos readback injected (new API). Probe suite
  694 to 700 (`item3-probe-green-full.log`).

## 4. Directories created for guest paths (`150c592b`)

- **0700.** Each missing directory of a guest path is created owner-only 0700
  before the umask; it was 0755
  (`crates/bridgevm-hvf/examples/hvf_gic_boot_probe/agent_console/share/guest_write_directory.rs`).
- **Decision: directories are marked too.** Gatekeeper's first-launch check
  reads the mark of the file it runs, which every guest file already
  carries, but an app bundle is a directory and LaunchServices reads the
  bundle's own mark [rank 4, not observed]. The mark is set through the new
  directory's descriptor before anything is published in it. A directory
  whose mark cannot be set is removed again and the write refused, and that
  refusal persists as in section 3. An existing directory is used as it is.
- **Tests** [rank 3]: `guest_write_directory_tests.rs`. On the unchanged
  source a created level was 0755 and a guest file was published inside a
  directory whose mark could not be set; the existing-directory pin passed
  (`item4-directories-red-individual.log`, one test at a time because a
  failing Fixture test poisons its lock). Probe suite 700 to 704.

## 5. Harness-mode GET (`6a2145e4`)

The finding was partly stale at `8d3eb052`: since `798451a4` the file goes
through the share's Destination, so it already had the file policy (0600, no
execute bit on an overwrite, mark before the rename, refused if the mark
cannot be set). Pins for that passed on the unchanged source. What was left
were the missing levels of the GET directory, which `create_dir_all` made 0755
and unmarked. They now get the directory policy of section 4; the deepest
existing ancestor is opened as configured, symlinks included
(`crates/bridgevm-hvf/examples/hvf_gic_boot_probe/agent_console/share/harness_get_dir.rs`).
Tests [rank 3]: `get_dir_policy_tests.rs`; on the unchanged source the created
levels were 0755 and a level without its mark was kept
(`item5-harness-get-red.log`). Probe suite 704 to 708.

## Checks

At each Rust commit: the probe example suite, `cargo fmt --all --check`, a
per-file stdin rustfmt comparison with the base (no hunk added), clippy for
the workspace and for bridgevm-hvf with venus, and the budget check on a clean
checkout of that commit. At each Swift commit: the four shim suites, with no
hand-listed swiftc source set naming a changed file. Budgets lowered:
`harness_protocol.rs` 507 to 500, `share_sync.rs` 307 to 283,
`guest_write_policy.rs` 94 to 86, `host_destination.rs` 144/8 to 140/7,
`harness_get_end.rs` 62 to 61, `T17ControlReply.swift` 27 to 26 and
`scripts/verify-keyboard-input.sh` 176 to 175.

## Not verified

- No live guest run: no Windows guest answered an operator CLIPGET, wrote a
  refused or a nested share file, or served a harness-mode GET under these
  changes. The A6 and A4 scripts and the T17 journey were not run.
- No Gatekeeper or LaunchServices behaviour for a marked directory or bundle.
- msdos, exfat, SMB, NFS and FUSE shares were not tried for directories.
- The guest-tools copy route was not exercised in the running app.

## Seen but not changed (for the parent)

The harness-mode GET line prints the guest path and the first 48 bytes of the
file in hex, the share's write-refused line prints the guest path, and the
CLI's VM status prints the guest-tools clipboard text.

## Registry impact (for the parent; the registry is unchanged)

A6's operator readback is now a length and SHA-256 prefix, and T17's
clipboard check is exact. A7's guest-created directories are 0700 and marked.
A11's probe example suite count is 708 on this branch, 693 at `8d3eb052`.
