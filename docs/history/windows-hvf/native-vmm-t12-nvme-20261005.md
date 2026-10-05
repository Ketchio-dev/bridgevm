# Native board T12 NVMe gate — 2026-10-05

**Live gate:** `t12-native-fb9cd083-20261005-r1` passed the unchanged fixed
20/20 requirement on exact `fb9cd0830f41f591b68faa6cf6a322494bb5aec7`.
It ran from 05:07:15 to 05:09:00 UTC on Mac16,9 / macOS 26.7 after that SHA's
local project check, exact hosted full run 37262227685 and CI 37262230042
passed. Hosted full had 43 passing stages and a conditional cross-target
skip; the separate CI passed all 14 jobs including Linux cross-compilation.

## Raw evidence and provenance

Root normalized and read all 20 lane logs. Each has two successful standard
UEFI PCI/NVMe reports: 40 firmware boots, 80 BAR-register reads and 40 Block
I/O reads of the expected marker. Reports have three modeled PCI functions,
completed enumeration, the required CAP/version/command values, present
512-byte media and last block 2047. Separate write/restore processes report
DXE results 10/11, variable states 1/2 and matching restored hashes.

All 20 vars files have distinct device/inode identities and are 64 KiB each.
The sorted relative-path hash aggregate was independently recomputed:
`2d89d90e7b6337e1194211112bbcc83311d4d1ef3df9a38939f993c41dfdec55`.
Retained firmware bytes and their pinned build receipt agree on SHA-256:
`6b041c345f6707d38b388d232b0a9ce63a5ba6153d31462f9071da9943571aa8`.
The build receipt pins EDK2 `b03a21a63e3bd001f52c527e5a57feddb53a690b`.

The producer recorded the signed binary's pre-run hash:
`2298a547883f73162ae019b2069717edd4c1843bf311c6e95f705565d4ecd0d4`.
Root's optional independent binary audit ran after worker cleanup and failed
to open the removed binary. No independent retained-binary hash or strict
signature verification was performed; the missed audit remains recorded.

The unchanged redacted [public receipt](../../windows-arm/evidence/native-vmm-t12-receipt-20261005.json)
has SHA-256 `2fa3f161929c289a9b27bd70155f9542b71880d68e07647b603d77c7fc1e0545`.
It carries the 20/20 verdict and input hashes, but the existing redactor omits
the four boot/read count fields. Counts above are supported by the private
producer receipt and all raw lane reports, not by those absent public fields.

Additional retained evidence hashes:

- Private producer receipt: `642d2685918c0e88d5b0f25d81ff265e84f3d6a90710531c886076a8da4dd12d`.
- Twenty-lane summary: `8423027d6f6b7f34a43b6732803082d25b965ad806d29d39f329af5b91a183fe`.
- Root raw-log/hash/cleanup audit: `20f0974ca02f24fdd555da1ea7cff63596ddd2b78ac70eccdc9690cb651201dc`.

## Scope and cleanup

Terminal result is pass/exit 0. Root confirmed no running or queued job,
remaining worktree, VM process, worker lock or cleanup fence; the worker was
idle with last exit 0. No guest disks or vars enter this repository.

This proves the independent board's standard UEFI PCI/NVMe Block I/O and
vars persistence across separate processes at the declared sample count.
It does not prove interrupt delivery, Windows boot or workload performance.
All registry criteria, product state and release boundaries remain unchanged.
The earlier `t12-native-c23d20ec-20261005-r1` pre-VM firmware refusal remains
a failed experiment; this is a distinct successor result.
