# A19 item 5: Windows boots with the restored original marker

## Scope and result

One full `t1-restore-boot` gate passed on physical hardware. This proves the
powered-off restore-boot marker sequence for the sealed runtime and inputs
below, not the other snapshot acceptance items or overall product readiness.
It does not establish RAM/device resume, performance superiority or 3D support.

- Job: `t1-restore-9989d0d9-native-shutdown-r3`.
- Commit: `9989d0d9fa9234f03e44d0bef92d6d81cdf2a28f`.
- Started: `2026-09-12T00:34:46Z`; finished: `2026-09-12T00:38:33Z`.
- Queue result: `pass`, exit code `0`; public receipt: `pass=true`, sample count `1`.
- Release build: `venus`, Rust `1.97.0`; exact-commit hosted CI passed before submission.
- [Frozen public receipt](a19-restore-boot-20260912-receipt.json).

## Observations, not merely an END count

| Phase | Read before write | Verified after write |
| --- | --- | --- |
| 1: before snapshot | `BV-NO-MARKER` | `BV-ORIGINAL-1789173328` |
| 3: after snapshot, before restore | `BV-ORIGINAL-1789173328` | `BV-CLOBBERED-1789173412` |
| 5: after restore | `BV-ORIGINAL-1789173328` | `BV-FINAL-1789173474` |

Each marker command required its exact fresh `exit=0` record and matching END.
The write commands also read back their value before reporting success.
All three boots then recorded `shutdown.exe /s /t 0 exit=0`, a 68,719,476,736-byte
NVMe writeback, and `stop: PSCI 0x84000008 (system off)`. Forced host cleanup or
residual guest processes cannot satisfy the gate. The queue completed normally.

Create, verify and restore reported the same snapshot disk/vars identities:

| Artifact | Bytes | SHA256 |
| --- | --- | --- |
| Snapshot disk | 68719476736 | `a8a3cccea9c1cadabbbb387c62e14bdbe1805b1bb832f3a10f3bd3a6c4f04597` |
| Snapshot vars | 67108864 | `bec224d27c8681d2db69583e933e2d99b6fa5265d91d37373cb7a2c8b71853cd` |

The receipt records source-image, source-vars, binary and manifest SHA256 values.
Canonical inputs stayed read-only; only newly created private working clones
received owner write permission. Disks and vars are not repository artifacts.

## Retained evidence identities

| Private evidence | SHA256 |
| --- | --- |
| Phase 1 run log | `4d18e3bedb5aeca241e5578fa82f5dd585dd1a3f5084fb25d328ba2bb37f7ff8` |
| Phase 3 run log | `fbe49b2760880de9bdc1f84b3c1562a8f66f8a300d7fa70b5da4861af489aa81` |
| Phase 5 run log | `356754f0c3420ef52633710c2abe188a522df471856302a896924cb4b78a2208` |
| Frozen public receipt | `9c482f4ee3ee97ed20c7c9eefce1abcc3b8ddfe5b746b9c2c9754d201d7d6c44` |

The operator observed `Mac17,9` / macOS `26.5` after completion; these host fields
are not part of this receipt's sealed identity. Raw logs remain in the private
queue's completed job directory and contain private paths, so are not published.

## Failed predecessors and historical qualification

`t1-restore-7efbdf15-sealed-r1` failed before boot: copied read-only permissions
prevented opening the private disk for NVMe writeback. The next run,
`t1-restore-05dead35-writable-r2`, booted and verified a marker but its legacy
`v3-share2` agent rejected `POWEROFF`; it was canceled and its receipt withheld.
Neither failure is converted into a pass by this later run.

The [August record](a19-snapshot-restore-20260805.md) remains historical evidence
with unavailable original logs and a weaker waiter. This fresh run does not
retroactively prove its clobber write. Full A19 closure still requires the
remaining [V1 acceptance items](../snapshot-scope-v1.md), not this single receipt.
