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

## 150-percent cell failure retained; caption-point correction pending live proof

Job `d2-b6-a399e341-1600x900-150-r1` measured the same a399e341 code and
1600x900 resolution with LogPixels=144. It ran from
2026-09-10T05:06:02.836693Z to 2026-09-10T05:17:06.324055Z and terminated
`valid=false`, `outcome=failed`, `failure_code=capture-failed`, `run_count=0`.
All promotion/pass flags remain false. Its first paired run and all three
packaged scenes collected data, but classic runs 2 and 3 failed foreground
acquisition and have absent captures; this is not a completed cell.

All reported scene windows had effective dpi=144 and monitor_scale=150. Classic
HWNDs 262534 and 393246 were reported at visible bounds x=84, y=90, width=1032,
height=741 before the WINBOUNDS request. The request `50 60 700 500` returned OK,
then all five fallback attempts for each window clicked HID `8196x2842`, the
hard-coded physical point `(400,78)`, and observed foreground HWND 65786 instead
of the target. No post-WINBOUNDS physical rectangle or hit-test was captured,
so the exact geometry explanation remains an inference, not a completed causal
proof. The fixed point is visibly scale-dependent and lacks target authentication.

Microsoft documents that GetWindowRect is DPI-virtualized while DWM extended
frame bounds are physical coordinates:
[GetWindowRect reference](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getwindowrect).
The correction queries physical bounds under per-monitor-v2 thread awareness,
requires an uncovered point owned by the exact target and `WM_NCHITTEST` result
`HTCAPTION`, and converts that verified point into the existing HID coordinate
range. The parent still verifies actual foreground after clicking. Host parser
contracts reject stale/ambiguous responses, wrong owners, non-caption hit tests,
offscreen coordinates and failed guest commands. Native Windows contracts and a
new sealed live attempt are separately required; source reasoning is not proof
that this fixes the observed guest failure. No glyph/performance threshold changes.

Failed receipt SHA-256:
`e40efa0c3b8d37b53f601953c44100143eb7fe11d48757710fbe3326889b2926`.
Capture-stage log SHA-256:
`381d843a55c9f1025d7086abe62fd0d67dc976a268c6ba9e2457f7ed37e0a443`.
Per-run records SHA-256:
`a68a4a4200f080dd4424e5f2c7c08ec3890024b5768cf924608f9b407fbd1744`.

Caption-point code commit `a993051cedebb5b8193f1527bddea9c2cc60fd6f` passed five local host-parser unittest
methods and the full local `scripts/check-project.sh`. This is deterministic
source/host validation only; native Windows and a revised live cell remain pending.

## Physical-caption fallback observed at 150 percent, under red documentation CI

Diagnostic job `d2-b6-4f984051-1600x900-150-r2` at exact commit
`4f984051897551a56393e9fb7dcda68bd4940fd3` completed from
2026-09-10T05:30:53.425528Z to 2026-09-10T05:42:09.886799Z. The receipt is
`valid=true`, `outcome=observed`, `failure_code=none`, `run_count=3` and
`required_run_count=27`; all pass/claim/promotion flags remain false.
Classic runs 2 and 3 authenticated physical point `(600,100)`, dpi=144,
`hit=2`, with owner HWND matching target HWND (458786 and 328248). The first
fallback click acquired actual foreground in each case, and both previously
missing classic scene paths collected their captures and FrameTime CSVs.
This supports the changed fallback on this observation, not universal focus
reliability or completion of the glyph/performance matrix.

Receipt SHA-256:
`40975f0026732ff3d474e1c3e82712cf0d17dc6936495869996883a06661ecc8`.
Cell-result SHA-256:
`0777966537b20cf0248e19c592d4cd5840b0a6714abd6f79d86f19e4dce13bcd`.
The source media/library/config hashes match the prior 150-percent attempt;
the harness code changed and each attempt used its own fresh disk/vars clones.

Exact 4f984051 hosted CI 34440966393 FAILED its documentation-reference step:
a worker Git blob identifier in the venue evidence was interpreted as a missing
commit. Security and all five B6 workflows passed, including native caption
contracts 34440966394. The failed CI is not waived: this live result is diagnostic
and is not promoted to release evidence. The introduced documentation notation
issue is awaiting operator approval to correct.

## First-run tip remains a separate layout-dependent problem

The failed 150-percent attempt's first packaged capture visibly retained the
first-run tip. Its `Got it` button occupied approximately x=518..729,y=235..281,
while the previous harness clicked `(544,302)`. The tip covers some editor text.
This is not a diagnosed renderer cause. Packaged editor text also appears
cyan/blue-fringed at both observed display scales; no healthy reference or
reviewed glyph mask establishes the cause or correctness.
Original 150-percent packaged-run1 PPM SHA-256:
`c982b22b0d02856ccb946381860589fd07183c17c69dfb964c2434f67474cba2`.
Original classic-run1 PPM SHA-256:
`7aee65dc58d5569de37e11c729598100a01fc4556dd1d9dc5d62d9141f91c606`.
Visual inspection used separate PNG conversions; original bytes were unchanged.

A new tip helper searches only descendants of the requested application window
for the English `Got it` button, requires one visible enabled same-process match,
and verifies actual window ownership at its physical center before the host
sends a HID click. It then queries again for disappearance. UIA non-discovery is
explicitly recorded as `not-found`, not visual proof of no overlay. English-name
and application-provider coverage remain limitations until real guest captures
are inspected. The public matrix and performance requirements are unchanged.
Microsoft documents physical UIA bounds and recommends a narrow application
search root rather than the entire desktop subtree:
[BoundingRectangle](https://learn.microsoft.com/en-us/dotnet/api/system.windows.automation.automationelement.boundingrectangleproperty),
[obtaining UIA elements](https://learn.microsoft.com/en-us/windows/win32/winauto/uiauto-obtainingelements).

Tip-target code `e02f9486a213ab0454aa753b8bbc89e62cd4fd38` passed five local host-parser/sequencing unittest
methods. Full local `scripts/check-project.sh` failed exactly one step,
`documentation references`, for the already-recorded blob notation issue.
Native UIA tests and a new live attempt remain pending. This is not a completed
project check or a release result.
