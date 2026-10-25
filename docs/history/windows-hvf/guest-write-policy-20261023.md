# Guest write policy: clipboard size and shared-folder files — 2026-10-23

Classification: two product decisions, implemented test-first in `07af5e4c`
(clipboard) and `f86b69f6` (shared folder). **No live run has exercised either
change**, and no criterion, registry entry or product state changes here.

Evidence ranks, as in `AGENTS.md`: **rank 3** is an automated test in the probe
example suite, and **rank 4** is static reasoning. A **host check** is a single
manual command run on this Mac (macOS 26.7) with its output kept outside the
repository. It is not a BridgeVM live run and ranks below rank 3. The red and
green test logs are kept with the host checks, outside the repository.

## Decisions

The parent agent made both decisions on 2026-10-23 under the operator's
explicit delegation. They are recorded as made.

1. **Guest-to-host clipboard size.** A guest clipboard text whose decoded size
   would exceed 8 MiB (8,388,608 bytes) is refused. The base64 length is
   checked before decoding. A refused text is never truncated and never
   written to the Mac pasteboard. Refusals are counted and reported in a
   bounded line that carries no content. Host-to-guest sync is unchanged.
2. **Files the guest writes into a shared host folder.**
   - (a) They never carry execute permission. A new file is created 0644
     before the umask. An overwrite keeps only the replaced file's read/write
     bits, so the setuid, setgid and sticky bits stay cleared.
   - (b) `com.apple.quarantine` is set on the temporary before the rename, as
     `0083;<8-hex unix seconds>;BridgeVM;<UUID>`, with the UUID from the host
     CSPRNG. If the mark cannot be set, the write is refused.

The worker made four further choices within those decisions:

- **ENOTSUP is refused.** On a share filesystem without extended attributes,
  the write is refused like any other failure to set the mark.
- **The mode is verified after it is set.** A temporary that still reports an
  execute, setuid, setgid or sticky bit is refused.
- **A refusal does not adopt the guest text.** After a refused clipboard text,
  the guest's text counts as unknown, and a pending rebaseline stays pending.
- **One refusal per guest text.** An unchanged guest clipboard is polled every
  interval, but it counts and prints once.

## 1. How guest clipboard text reaches the Mac pasteboard

The Mac pasteboard is written by the probe process itself, not by the app.

| Step | Where | Now |
| --- | --- | --- |
| The guest replies `CLIP <base64 of UTF-8>` to every poll | `bvagent.ps1` | unchanged |
| The line is framed, at most 32 MiB | `MAX_AGENT_LINE_BYTES` in `harness_protocol.rs` | unchanged |
| The reply is decoded and admitted | `crates/bridgevm-hvf/examples/hvf_gic_boot_probe/agent_console/clip_guest_admission.rs` | size refusal |
| An adopted text is queued for the Mac | `clip_sync.rs`, `HostPasteboard::set` | unchanged |
| The pasteboard thread writes it | `Poller::apply_set` in `crates/bridgevm-hvf/examples/hvf_gic_boot_probe/host_pasteboard_write.rs` | refuses over 8 MiB |
| The native write | `NsPasteboardBackend::set_string` | unchanged |

Before this change, a reply up to the 32 MiB line limit decoded to about
24 MiB, and lossy UTF-8 decoding could triple that. [rank 4]

Two other routes were traced and left unchanged:

- An operator `CLIPGET` through the control file prints the decoded guest text
  to `run.log` in `handle_reply_line`. It never writes the pasteboard.
  [rank 4]
- In the app, `copyGuestClipboardToHost` writes a guest-tools snapshot to the
  pasteboard when the user presses "Copy to Mac clipboard". The app decodes
  that snapshot from `last_clipboard`, but the daemon serializes the field as
  `clipboard`. The route is therefore unreachable today, and it has no size
  bound. [rank 4] The app's only other pasteboard write copies a command line
  the app built itself.

## 2. What the clipboard refusal does

Each claim below is rank 3 and cites its test in
`crates/bridgevm-hvf/examples/hvf_gic_boot_probe/agent_console/clip_guest_admission_tests.rs`.

- **The threshold holds at the boundary.** A guest text of 8 MiB + 1 byte
  never reaches the pasteboard, and an 8 MiB text is adopted whole
  (`guest_text_one_byte_over_8_mib_never_reaches_the_mac`,
  `guest_text_of_exactly_8_mib_syncs_untruncated`).
- **Size is checked before decoding.** A payload is refused when its base64
  length implies more than 8 MiB, before it is decoded. A non-base64 payload
  of that length is refused and counted, whereas decoding it would have
  failed to empty text (`oversized_payload_is_refused_by_its_base64_length_before_decoding`).
  The bound accounts for padding, so it is exact for valid payloads
  (`base64_length_gives_the_exact_decoded_size`).
- **Lossy growth is measured.** The size after lossy UTF-8 decoding is computed
  before that text is built. Raw bytes within 8 MiB that would exceed it after
  U+FFFD replacement are refused, and exactly 8 MiB after replacement is
  adopted (`invalid_utf8_growing_past_8_mib_is_refused_not_truncated`).
- **The Mac keeps its copy.** After a refusal, a later guest copy is adopted
  and a later Mac copy reaches the guest. A Mac copy equal to the text from
  before the refusal is pushed, because the guest no longer holds it
  (`after_a_refusal_later_copies_sync_both_ways`,
  `after_a_refusal_the_guest_text_is_unknown_so_any_mac_copy_is_pushed`).
- **A pending rebaseline stays pending.** If a legacy paste was written and its
  `Set-Clipboard` lands after the refused poll, the pasted text becomes the
  baseline and is not published
  (`refusal_keeps_a_pending_rebaseline_so_a_late_paste_is_not_published`).
- **Each refused text counts once.** The count rises once per refused text,
  however often it is polled. It rises again after any admitted reply or new
  baseline (`each_refused_guest_text_is_counted_once`).
- **The record line carries no content.** It reads
  `BVAGENT CLIPSYNC refused guest-text too-large bytes=<n> limit=8388608 refusals=<k> t=<ms>`,
  at most 160 bytes. It names no direction, and the test checks that
  `BvAgentEvent.swift` reads a CLIPSYNC line as a sync only when it names one
  (`refusal_line_is_bounded_and_carries_no_clipboard_content`).
- **The pasteboard thread also refuses.** It never writes guest text over
  8 MiB, whoever queued it
  (`pasteboard_worker_never_writes_guest_text_over_8_mib`).
- **Host-to-guest sync is unchanged.** The existing clipboard tests pass
  unchanged, including those for the `clip_max_bytes` limit (8 KiB default).
- **Test results.** On the unchanged source, 6 of the 7 behaviour tests
  failed. The exactly-8-MiB test passed, as it should. The 4 record tests did
  not compile. After the fix, the probe example suite passed 663 tests
  (652 + 11).

The guest still sends an unchanged oversized clipboard at every poll, up to
the 32 MiB line limit. The host hashes it to recognise it, but no longer
decodes it. The cost of that hash on the service tick was not measured.
[rank 4]

## 3. Files the guest writes into the shared folder

A guest file reaches the host through these steps:

1. `handle_share_get_end` in `share_host_write.rs` receives it.
2. `Destination::write` in `host_destination.rs` writes it to an exclusive,
   no-follow temporary.
3. `seal` in
   `crates/bridgevm-hvf/examples/hvf_gic_boot_probe/agent_console/share/guest_write_policy.rs`
   prepares that temporary.
4. The temporary is synced and published with `renameat`.

Unless marked otherwise, each claim is rank 3 and cites its test in
`crates/bridgevm-hvf/examples/hvf_gic_boot_probe/agent_console/share/guest_write_policy_tests.rs`.

- **New files are 0644 before the umask.** A new guest file gets the mode of
  any 0644 create under the process umask, with no execute, setuid, setgid or
  sticky bit. It was 0600 (`new_guest_file_is_created_0644_before_the_umask`).
- **An overwrite keeps only read/write bits.** Overwriting a 0755, 6755, 0711
  or 0640 file leaves that mode masked with 0666. The old code masked with
  0777, so it kept execute bits
  (`overwrite_keeps_only_the_replaced_files_read_write_bits`).
- **An execute ACL cannot make the file run.** The replaced file's ACL is still
  copied onto the guest's file; the deny-ACL test below depends on that copy.
  Guest content written over a 0755 file with an "everyone allow execute"
  entry still cannot be run, because `exec` needs an execute mode bit. On the
  unchanged source, that content ran
  (`guest_content_over_an_executable_with_an_execute_acl_cannot_run`).
- **The mode is verified.** A temporary that still reports any of those bits is
  refused, and no mark is set (`a_temporary_that_keeps_execute_bits_is_refused`).
- **FAT and exFAT shares refuse guest writes.** On both, every file reports
  rwx------, before and after chmod 0644, and a script placed there ran
  [host check]. Guest-to-host writes into a share on either filesystem are
  therefore refused. Host-to-guest sync does not use this path.
- **Published files carry a fresh mark.** Every published guest file carries
  `0083;<8 lowercase hex seconds>;BridgeVM;<UUID>`. The UUID is version 4 with
  the RFC 9562 variant, from `SecRandomCopyBytes`, and two writes get
  different UUIDs (`published_file_carries_a_fresh_quarantine_mark`,
  `mark_is_flags_hex_seconds_agent_and_a_version_4_uuid`).
- **The format matches the system's own marks.** This Mac's own marks have the
  same `flags;hex-seconds;agent;UUID` form, such as Safari's `01c3` marks
  [host check]. Foundation parsed a mark in this form into agent name
  `BridgeVM`, its timestamp and the UUID as event identifier [host check].
  The SDK's `LSQuarantine.h` documents the quarantine properties, not the
  attribute string or its flag bits.
- **The mark is set after the attribute copy.** `fcopyfile` re-applies the
  replaced file's own quarantine to its destination, as Apple's `copyfile-240`
  source shows [rank 4]. A replaced file's approved mark therefore came back on
  the unchanged source, rewritten as `03c1` with an empty agent. The mark is
  now set after the copy, and other attributes are kept
  (`overwrite_replaces_an_approved_mark_and_keeps_other_attributes`).
- **A leaf symlink swap cannot move the mark.** The mark is set through the
  temporary's descriptor, so a symlink swapped in at the leaf does not carry
  it to the link's target
  (`leaf_swap_cannot_move_the_mark_onto_the_link_target`).
- **A mark that cannot be set refuses the write.** The old file stays and the
  temporary is removed. This holds for a real `fsetxattr` failure on a new file
  (`a_new_file_whose_mark_cannot_be_set_is_not_published`). It also holds for
  an overwrite whose ACL, copied onto the temporary, denies writing extended
  attributes (`an_overwrite_whose_mark_cannot_be_set_keeps_the_old_file`).
- **ENOTSUP, a missing CSPRNG and the refusal path.** ENOTSUP is refused as
  "share filesystem has no extended attributes"; this is tested with an
  injected errno only
  (`share_filesystem_without_extended_attributes_refuses_the_write`). A CSPRNG
  with no bytes refuses the write, with no fallback UUID
  (`csprng_failure_refuses_the_write_without_a_fallback_uuid`). Each refusal
  is printed through the existing `BVAGENT SHARE write-refused` line.
- **No ENOTSUP on FAT or exFAT.** Neither filesystem returns ENOTSUP: macOS
  stores the mark in an AppleDouble `._` file [host check].
- **The existing protections are unchanged.** The no-follow opens, the
  exclusive temporary with `renameat` and the symlink protections are
  unchanged, and the 8 existing `host_destination_tests.rs` tests pass
  unchanged.
- **Test results.** On the unchanged source, the 8 tests that go through
  `Destination` failed. The 3 tests of the policy's own functions did not
  compile. The kept-execute-bit test failed against the first
  implementation, before the verification existed. After the fix, the probe
  example suite passed 675 tests (663 + 12).

## Not changed

- **Created directories.** The directories a guest path causes to be created
  are still created 0755, and they carry no mark. The guest files inside them
  are 0644 and marked.
- **Retries of persistent refusals.** A refused write still takes the existing
  failure path: the file is fetched again at each guest listing, and the
  share is disabled after 4096 failed names [rank 4]. For refusals that cannot
  succeed (FAT or exFAT, ENOTSUP, a deny ACL), the retries are futile work.
  This remains a follow-up.
- **Harness-mode GET.** `finish_get` writes through `std::fs::write` with no
  mode or mark policy. It is not the shared folder.
- **Other clipboard routes.** The guest-tools clipboard route and the operator
  `CLIPGET` in section 1 are unchanged.

## Not verified

- **No live run.** No Windows guest has sent an oversized clipboard text or
  written a share file under these changes.
- **No Gatekeeper behaviour.** No guest-written file was opened in Finder or
  LaunchServices, so the Gatekeeper prompt for the mark is not observed.
- **Untested filesystems.** SMB, NFS and FUSE shares were not tried. ENOTSUP
  is tested only by injection.
- **No measured cost.** The time spent hashing a refused oversized reply was
  not measured.

## Registry impact (for the parent; the registry is unchanged)

`capabilities/windows-hvf.json` evidence paths do not include any changed
file. These criteria and wordings describe behaviour that changed:

- **A6, Clipboard** (PROVEN, release-blocking). Statement: "Host and guest
  clipboard round-trips in both directions including Korean text with matching
  SHA-256." Guest-to-host texts over 8 MiB are now refused.
- **A7, Folder sharing** (PROVEN, release-blocking). Statement: "One file each
  direction transfers with matching SHA-256." Guest-written files are now
  created 0644 before the umask, or take the replaced file's mode masked with
  0666, and they carry a mark.
  They are refused on FAT and exFAT, on filesystems without extended
  attributes, and where the mark cannot be set.
- **`wording.engine_summary`.** "... clipboard and folder integration ..."
  carries none of these limits.
- **A11, No regression** (OPEN). Its measured text cites "the probe example
  suite 652"; this branch has 675. Its exact-SHA receipt cannot cover
  unsealed commits.
