# Sealed B6 cell observations

Date: 2026-09-10. Code: `020b97dd`. This adds an observation venue,
not a B6 release gate. B6 remains OPEN.

## Execution boundary

`d2-b6-cell-observation` runs through the physical-Mac queue at the submitted
commit. Submission copies and hashes the input manifest and probe binary.
The tier verifies exactly eight inputs: image, vars, binary, virglrenderer,
MoltenVK, driver store, PresentMon, and cell configuration.

Configuration permits only 1280x720, 1600x900, or 1920x1080 with LogPixels
96, 120, or 144. It cannot override the existing three paired scene runs.
An individual cell is not the original 27-run matrix.

## Media and failure discipline

- Canonical disk and vars must be read-only, authenticated regular files.
  Symlinks, unsafe driver-store entries, changed hashes, and aliased media
  are refused.
- A unique private work directory is required. Cross-volume inputs are
  refused rather than silently copied as if they preserved APFS cloning.
- The disk uses `cp -c`; vars receive their own copy. Both destination
  identities and hashes are checked before making the copies writable.
- Scale setup and scene capture use separate launcher processes on those
  copies. Source hashes are checked again after successful capture. Final
  clone hashes are recorded and successful retained copies become read-only.
- A false, incomplete receipt is written before input validation or live
  work. Failures retain their stage and private diagnostic; the existing
  worker owns process-group cleanup before publication.

Private media and guest content remain outside git and CI artifacts. The
public receipt uses hashes, counts, and relative evidence references. Its
`gate_asset_hash` identifies the sealed PresentMon executable. Existing
binary preflight also checks signature, Hypervisor entitlement, compiled 3D
support, and the exact linked renderer path.

## What success means here

The tier rechecks all three paired scene records and all six authenticated
FrameTime reports. It can report `valid=true`, `outcome=observed`, and
`run_count=3`. Even then, `pass`, `claim_eligible`, `criterion_pass`, and
`capability_promotion` remain false, with `required_run_count=27`.
Queue command completion is not criterion completion.

This tier supplies no reviewed glyph masks or accepted performance baseline.
It does not prove that a partial cell closes B6, that different probe binaries
are comparable, or that input acknowledgments prove rendered glyphs. Original
matrix and within-10-percent requirements remain unchanged.

## Deterministic evidence

`tests/integration/b6-cell-observation-contract.py`: 15 tests passed locally
in 0.281 seconds. They cover the allowed configuration, forbidden overrides,
input changes, writable canonical media, manifest shape, symlinks, sealed
binary mismatch, copy command shape and independent vars, reused lanes,
hardlink refusal before chmod, false initial/failure receipts, and queue
copying/sealing. Fixture copies do not prove APFS clone relationships,
Windows bootability, CGL behavior, or a live B6 result.

Preceding `877de4e1f671eef4e5258e9e53a9f88541357c69` has green
[CI 34436476234](https://github.com/Ketchio-dev/bridgevm/actions/runs/34436476234),
[Security 34436476196](https://github.com/Ketchio-dev/bridgevm/actions/runs/34436476196),
[collector 34436476190](https://github.com/Ketchio-dev/bridgevm/actions/runs/34436476190),
[FrameTime 34436476227](https://github.com/Ketchio-dev/bridgevm/actions/runs/34436476227),
and [active collection 34436476218](https://github.com/Ketchio-dev/bridgevm/actions/runs/34436476218).
The new tier requires its own full project check, exact-checkpoint hosted
results, and a recorded physical-Mac job. No live result is claimed here.

## First sealed live cell: observation, not criterion completion

Job `d2-b6-a399e341-1600x900-100-r1` measured exact commit
`a399e34103f52bfc66c5a1fb7935d762676dcb5f` on Mac17,9, macOS 26.5.
The tier ran from 2026-09-10T04:41:45.589322Z through
2026-09-10T04:52:54.236945Z. Queue state is `done`; receipt says
`valid=true`, `outcome=observed`, `failure_code=none`, `run_count=3`,
`required_run_count=27`, and `pass=false`, `claim_eligible=false`,
`criterion_pass=false`, `capability_promotion=false`.

This is one 1600x900, LogPixels=96 cell with three paired classic/packaged
scenes. All six effective-window checks reported dpi=96, awareness=2 and
monitor_scale=100. Classic runs 2 and 3 needed the existing HID focus fallback.
This does not establish continuous focus or glyph correctness.

| Scene | FrameTime rows | Mean ms | Nearest-rank p95 ms | Host input commands |
| --- | ---: | ---: | ---: | ---: |
| Classic 1 | 43 | 321.268033 | 516.2004 | 33 |
| Packaged 1 | 80 | 180.029775 | 384.9956 | 33 |
| Classic 2 | 44 | 323.143250 | 515.2380 | 34 |
| Packaged 2 | 76 | 188.409875 | 456.2833 | 33 |
| Classic 3 | 65 | 212.591389 | 490.5136 | 33 |
| Packaged 3 | 88 | 164.269390 | 372.9354 | 33 |

The 396 rows were authenticated against fresh guest-reported SHA-256 values.
These are diagnostic intervals under paced input, not throughput, an accepted
baseline, or a proof of the fixed within-10-percent regression clause. Idle
intervals and differing caret behaviour can affect them. The other eight cells,
reviewed pixel masks and equivalent accepted baseline remain outstanding.

Public receipt SHA-256:
`093f6606465c9643ed87178c9de78b6dcfb22c215fe79270b2b75779a70d5047`.
Cell-result SHA-256:
`5327dc4aad4c74a6f5c44126b874a0375a33e85a71bc806a631666da3720ce1b`.
Input-manifest SHA-256:
`0cf4e2347f27e2059b80acd7c4bde26943bd27ebbe22a803e5f3ad13290111d7`.
Config SHA-256:
`560f4753a662c0698c6a074c2f57909e89ef1df5a9d46769cdcba6c9db2446f9`.
The canonical source disk/vars were reauthenticated after the run, and each
private final clone was made read-only. The immediately preceding full source
hash is a warm-cache confounder. No private media or title content is published.

Visual review used separate PNG conversions of the original PPM captures;
original evidence bytes were not modified. Classic caption and menus were
legible. Packaged editor-body text appeared cyan/blue-fringed and thin or
fragmented, unlike its black menu text. This is an unresolved visual observation,
not a diagnosed rendering defect or proof of correct glyphs. No healthy reference
or reviewed mask was available. A horizontally clipped classic text prefix is
not by itself evidence of missing glyphs.
Original classic-run1 PPM SHA-256:
`ca5ed137eccf375ed55ff07f5109f4f96d1b98f0a89b82d4adab27363034ce8b`.
Original packaged-run1 PPM SHA-256:
`c85962a37f7a3d3f532b58985294ce2997428133897a4051b5a7d185c9876a93`.

Exact a399e341 hosted checks are green: CI 34437681356, Security 34437681408,
collector 34437681409, FrameTime 34437681437, active collection 34437681468,
sealed cell 34437681395. These results support that sealed observation only;
B6 and the product state are not promoted.
