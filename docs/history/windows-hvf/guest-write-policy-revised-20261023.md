# Guest write policy, revised the same day — 2026-10-23

Classification: corrections to
`docs/history/windows-hvf/guest-write-policy-20261023.md`, implemented
test-first in `9c9b54b9`, `bcdcef0f` and `c16940dc`. That record is left as
written. It is at its structural-budget ceiling, so this section could not be
appended to it. **No live run has exercised these changes**, and no
criterion, registry entry or product state changes here. Ranks and the
meaning of a host check are as in that record.

## Revised the same day

The parent corrected two of its own decisions after reading the first report.

1. **New guest files are owner-only 0600 again** (`9c9b54b9`). The 0644
   decision made guest-written files readable by every other local user. That
   was a confidentiality regression against the earlier 0600. A new file is
   created 0600 before the umask, with no execute bit. Overwrites are
   unchanged: the replaced file's mode masked with 0666 [rank 3,
   `new_guest_file_is_created_owner_only_0600`; it read 0644 on the unchanged
   source].
2. **msdos and exfat shares accept a guest write that carries a mark**
   (`c16940dc`). `f86b69f6` refused every guest write on these filesystems.
   They report every file executable whatever chmod asks, so the execute bit
   is not a boundary there. The rule is now: if the temporary still shows
   execute bits after the chmod, and the `f_fstypename` that `fstatfs` gives
   for the destination directory's volume is exactly `msdos` or `exfat`, the
   write is accepted only if its quarantine mark was set and reads back byte
   for byte. On every other filesystem, a leftover execute, setuid, setgid or
   sticky bit still refuses the write. On msdos and exfat, a leftover setuid,
   setgid or sticky bit also refuses it. ENOTSUP still refuses the write.

The parent also asked for a follow-up that is not a decision change.

3. **A refused clipboard reply is fingerprinted from at most 8 KiB**
   (`bcdcef0f`). The count-once fingerprint hashed the whole refused payload at
   every poll, and a payload can be up to 32 MiB. It now hashes the length
   and the first and last 4 KiB. A collision costs one uncounted refusal,
   never an admitted text [rank 3, `clip_guest_fingerprint_tests.rs`: 12,582,913
   bytes of a 9 MiB refusal were hashed before, at most 8,256 after].

### Why msdos and exfat, and the limitation

On these volumes the mark is stored in an AppleDouble `._` file [host check].
The parent's rationale is that Gatekeeper then checks the file when it is
opened through LaunchServices. That is Apple's described quarantine behaviour,
not observed here [rank 4]. Before `f86b69f6`, the code published these files
executable and unmarked; now they carry a mark. Compared with `f86b69f6`,
which refused them, the guarantee is weaker. **Every guest-written file on an
msdos or exfat share is executable by its owner (0700), new or overwritten.**
The mark is the only barrier left. Nothing here shows that it stops a process
that runs the file directly. Anything that deletes the `._` file removes the
mark.

### Behaviour by share filesystem

The filesystem type is read only when bits are left after the chmod, and the
mark is read back only on the msdos and exfat path.

| Share volume | Mode after chmod | Mark | Result |
| --- | --- | --- | --- |
| chmod holds (APFS) | new 0600; overwrite old & 0666 | set | published |
| chmod holds (APFS) | as above | not set (ENOTSUP or other) | refused |
| `msdos`, `exfat` | execute bits only (0700) | set, reads back exactly | published 0700 |
| `msdos`, `exfat` | execute bits only | not set, unreadable or different | refused |
| `msdos`, `exfat` | setuid, setgid or sticky | not attempted | refused |
| any other type | any leftover bit | not attempted | refused |
| any | any | no CSPRNG bytes | refused |

Rank 3 for every row, with the type and mode injected
(`crates/bridgevm-hvf/examples/hvf_gic_boot_probe/agent_console/share/guest_write_filesystem_tests.rs`).
The other types tried were `smbfs`, `nfs`, `macfuse`, an empty name,
`MSDOS`, `exfat2` and `msdos ` with a trailing space. On scratch msdos and
exfat images [host check], new files were published with a fresh mark and a
new UUID each. An overwrite replaced an approved mark with a BridgeVM mark and
kept the other attributes. The files stayed 0700, and the real `fstatfs` read
`msdos` and `exfat`. A failing `fstatfs` refuses the write
[rank 4, not tested].

### Reading the earlier record

- Where it says 0644 for new files, read 0600 (0700 on msdos and exfat).
- "FAT and exFAT shares refuse guest writes" no longer holds. The rule above
  applies, and such writes are no longer among the persistent refusals.
- The mode-verification, CSPRNG and ENOTSUP tests moved to
  `guest_write_filesystem_tests.rs`. The kept-execute-bit test is now
  `a_real_temporary_that_keeps_execute_bits_off_fat_is_refused_unmarked`.
- The fingerprint cost is now bounded by construction. It is still not timed.
- Registry impact, for the parent; the registry is unchanged. A7's
  limitations now read: created 0600 before the umask, or the replaced
  file's mode masked with 0666, and marked. On msdos and exfat, files are
  published executable by their owner with a mark that reads back. A11's
  probe example suite count is now 683. It was 675 before these commits,
  and 675, 677 and 683 after each.

### Not verified

- No Gatekeeper behaviour for a marked file on msdos or exfat.
- No real binary run of the old refusal on msdos or exfat. The old behaviour
  rests on the first round's host check.
- SMB, NFS and FUSE shares are still untried.
