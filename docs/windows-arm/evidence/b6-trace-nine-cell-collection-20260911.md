# B6 renderer-trace collection across nine cells, 2026-09-11

Historical diagnostic evidence, not a release gate pass. B6 remains OPEN.
Every job below ran the `d3-b6-renderer-trace` diagnostic tier. Its receipts
carry pass=false, criterion_pass=false, claim_eligible=false and
capability_promotion=false by construction. No reviewed glyph masks and no
accepted frame-time baseline exist for these runs, so they are not gate
evidence.

## Scope

Nine jobs at source `bd4e43a813cd2b0e789d401df25624fe492673dd` each ran one
declared cell: 1280x720, 1600x900 and 1920x1080 at LogPixels 96, 120 and 144
(100, 125 and 150 percent). Each completed three paired runs, a classic and
a packaged Notepad scene per run, for 27 runs. A tenth job at source
`c389c9b415095414ed836dc3ae0aba82f2097d3f` repeated the 1920x1080 100-percent
cell for three more paired runs. The intervening source includes 1f3f20ed
(render-server hash kept in public receipts) and 0afe4eb3 (channel
acknowledgment required before command success).

These are ten single-cell diagnostic jobs, not one matrix run. Every receipt
reports run_count=3 against required_run_count=27 and lists "Single cell, not
the 27-run matrix" as a confounder. The 1600x900 100-percent job was already
recorded in the
[renderer runtime reassessment](b6-renderer-runtime-reassessment-20260911.md).
The failed trace-less attempt `d3-b6-bd4e43a8-sealed-runtime-100-r1` is
recorded there and is not counted here.

All 17 hosted workflows at bd4e43a8 and all 18 at c389c9b4 passed before the
corresponding submissions. At c389c9b4 these include CI 34588479878, Security
34588479760 and renderer runtime contracts 34588479779.

## Sealed inputs

All ten input manifests carry the same hash for each shared input below. Each
receipt's identity fields match its manifest. Only the per-cell configuration
differs; both 1920x1080 100-percent jobs used configuration `3fd4d47f` and
the same manifest `bdfaeefe`.

| Input | SHA-256 |
| --- | --- |
| Image | `0bee49d3ea6b7c385336ba7f79183715deb8f3107c52ee7694930863b8d89a5a` |
| Vars | `bec224d27c8681d2db69583e933e2d99b6fa5265d91d37373cb7a2c8b71853cd` |
| Probe binary | `69401196ba6c5d7b72de6b52283ccaa3319c99ab9a07c0ff5f360c0f1ec70cf5` |
| virglrenderer | `13d913c57c1b3599920313927ab74d59dee618d8777c14cbf95551546f852354` |
| MoltenVK | `e1773b594b468796c6aacde6b8ec6f414315d94885c69122b56f45f3acebff93` |
| Driver store | `c46486e5643317002b8848ac855324bc397e6ee6ac6e2987da7af0c61c6a3860` |
| PresentMon | `9bec3083069f58f911e6a512f4806db51a27bd096103087bc1d05ef54c80a191` |
| Render server | `f63b98ad6f74d2fa86a040ad8746c2193ecd1fb6aa72d654f010ee1eceedf108` |
| Trace policy | `0f221af88e87895e3f5261bba36b4f78c9748a295715a8a931b3d670a346435c` |

Each preflight log names a disk and vars clone in a job-named work
directory. The ten final disk hashes are distinct. Final
vars hashes take one of two values, `0167bddd` in seven jobs and `9ecd7878`
in three; this is recorded, not diagnosed.

## Per-job results

The cited file is each job's full `receipt.json`. The bd4e43a8
`receipt.public.json` files omit render_server_sha256. The c389c9b4 public
receipt is byte-identical to its `receipt.json`.

Every receipt reports valid=true, outcome=observed and failure_code=none.
Each `capture/runs.json` shows pass or present for DPI, capture, focus and
PresentMon in both scenes of all three runs.

| Job | Cell | `receipt.json` SHA-256 | Startup-failure lines | Scale TGSI/GLSL | Capture TGSI/GLSL |
| --- | --- | --- | --- | --- | --- |
| `d3-b6-bd4e43a8-sealed-runtime-trace-100-r1` | 1600x900 96 | `91b25f06b8f65feaa6d0163ec184e7cd2de9c54fc2df00c1d068055972e2cb38` | 0 | 760/760 | 273/273 |
| `d3-b6-bd4e43a8-sealed-runtime-trace-125-r1` | 1600x900 120 | `1d4acd981d026cd64103457166c6108a78fcee3ab3103eacaab4ad5e17881bf0` | 0 | 748/748 | 272/272 |
| `d3-b6-bd4e43a8-sealed-runtime-trace-150-r1` | 1600x900 144 | `eea310189fe8e0e3afc4553197f8870a8f376ce99fd5b50a196e2e07d3aa81bc` | 0 | 735/735 | 266/266 |
| `d3-b6-bd4e43a8-sealed-runtime-trace-1280x720-150-r1` | 1280x720 144 | `378fb9c324559236b34ebdfc732021e3647a965221cbfb9664f7f741f7a91d04` | 0 | 736/736 | 279/279 |
| `d3-b6-bd4e43a8-sealed-runtime-trace-1280x720-125-r1` | 1280x720 120 | `57bf71862cb0c9cf24af686698cd97159e1d0c5710ff3024f9cfeaa3fa957f60` | 0 | 736/736 | 287/287 |
| `d3-b6-bd4e43a8-sealed-runtime-trace-1280x720-100-r1` | 1280x720 96 | `e79ea6f2be2e17bef6a7bbc201c94101ff74c4f4a7f6ff897925d8e77e473888` | 0 | 749/749 | 285/285 |
| `d3-b6-bd4e43a8-sealed-runtime-trace-1920x1080-100-r1` | 1920x1080 96 | `0506d72b209e005a23cf2fcb3674a55e4d8c3fb8d2cdf9abf4fb818c22bc4a11` | 0 | 751/751 | 271/271 |
| `d3-b6-bd4e43a8-sealed-runtime-trace-1920x1080-125-r1` | 1920x1080 120 | `844d03a36e7e9d1519edd076254d31ad957a77d41d07d16dbe873b0c3efb8237` | 0 | 748/748 | 272/272 |
| `d3-b6-bd4e43a8-sealed-runtime-trace-1920x1080-150-r1` | 1920x1080 144 | `a18f864a8f47c28a13747ac3cc7333b61fd4c10af871323d9937b172bf4e5219` | 0 | 748/748 | 276/276 |
| `d3-b6-c389c9b4-ack-contract-1920x1080-100-r1` | 1920x1080 96 | `58eeccbc1203f852376cf3dce69b11910de525b40cb8f36f43481f6e05246a5e` | 0 | 748/748 | 270/270 |

Jobs are listed in queue order. The first started at 2026-09-11T08:11:07Z;
the last finished at 2026-09-11T10:34:24Z.

Startup-failure lines were counted over both stage logs, `scale/run.log` and
`capture/run.log`, 20 logs in all. A line counts when its stripped text starts
with `proxy: failed to exec ` or `failed to initialize venus renderer`, the
prefixes `scripts/live-gates/b6_renderer_trace.py` rejects. A broader
case-insensitive search for either phrase also found none. Recomputed log
hashes and TGSI/GLSL counts match every `renderer-trace-summary.json`, and
each receipt's raw_sha256 matches its summary file.

## Confounders

- The sealed tier sets VREND_DEBUG=shader,cmd,obj,d3d in all ten jobs.
  Renderer logging may perturb execution.
- The virglrenderer build options record buildtype=release, b_ndebug=false
  and check-gl-errors=true. The recorded comparison against the 2026-09-10
  diagnostic build differs only in the installation prefix.
- Every receipt records a full clone integrity hash immediately before boot,
  which leaves the disk warm in the host cache.
- Every receipt records macOS 26.5 on host model Mac17,9. One host and one
  macOS version were exercised.
- Every capture log records two SetForegroundWindow refusals recovered by a
  verified caption click. Each also records two first-run tips whose UIA
  button was not found, marked as still needing visual review. No visual
  review is recorded here.

## Not established

- No reviewed independent glyph mask, and no mask verdict for any capture.
- No accepted frame-time baseline. PresentMon files were collected but not
  compared against one.
- Zero startup-failure lines only shows the two known signatures are absent.
  It does not prove the readiness handshake the reassessment found missing.
- B6 remains OPEN, and its known defect wording is unchanged.
