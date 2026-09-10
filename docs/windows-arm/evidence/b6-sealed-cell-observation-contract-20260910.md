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
