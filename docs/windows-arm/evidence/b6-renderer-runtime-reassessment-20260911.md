# B6 renderer runtime reassessment, 2026-09-11

Historical diagnostic evidence, not a release gate pass. B6 remains OPEN.
The required 27-run matrix, reviewed independent glyph masks and accepted
frame-time baseline are not established by this record.

## Correction to earlier observations

A read-only reassessment of 11 retained jobs found explicit renderer-server
startup failures in all 11. Nine had originally reported valid collections.
Those original receipts and framebuffers remain unchanged. Their completed
captures do not establish healthy Venus server initialization.

The reassessed jobs comprise all six 1280x720 and 1920x1080 scale cells at
79243f5c, its 1600x900 100% and 125% cells, and all three 1600x900 scale cells
at f915b52f. This supersedes any startup-health inference drawn from the
[earlier physical UIA observations](b6-physical-uia-observation-20260911.md),
not the existence of their recorded pixels or input events.

The candidate library embedded an uninstalled server path. The failed 125%
job exited with probe status 101. A completed 100% job also logged failed
server execution before reporting the backend enabled. Static inspection
found that proxy client initialization sends an INIT request without waiting
for a readiness response. Timing-dependent acceptance is consistent with the
logs; no complete readiness-handshake fix is claimed.

## Fail-closed checks and complete runtime

Source 4ba92627 rejects explicit server-exec and Venus-initialization failures
in either stage log and clears the valid sample count on errors.
Source 8d35615b requires a ninth sealed input, the render server, records its
hash, and checks an immutable executable with an exact embedded server path.
It rejects a server-path environment override. These are packaging checks,
not proof of a functioning Vulkan workload or all dynamic-library provenance.

The candidate was rebuilt with an installed process-mode server rather than
switching server modes. The first build omitted the prior b_ndebug=false
setting. Its three captures completed, but the mandatory shader traces were
absent, so the diagnostic correctly failed. This failed attempt is retained:
d3-b6-bd4e43a8-sealed-runtime-100-r1.

A separate trace-enabled build explicitly preserved b_ndebug=false. Comparing
reported Meson options against the original diagnostic build found only the
installation prefix differed. No existing candidate was overwritten.

## Subsequent bounded observation

Job d3-b6-bd4e43a8-sealed-runtime-trace-100-r1 finished at
2026-09-11T08:22:17.674041Z. At 1600x900 and 100%, it collected three paired
scenes. Both authenticated logs passed explicit startup-error screening.
The capture log contained 273 TGSI and 273 GLSL headers; the scale log
contained 760 of each. The receipt reports valid=true, outcome=observed,
run_count=3, and criterion_pass=false.

The sealed input manifest SHA256 is
a2bac8ff0cdc9e8a3639a82090dda64a002eeab98e63afca9b958aa27301eafd.
The capture log SHA256 is
390576be4570b09f7d18e45c6bf7b48fcaf588ec1449069238ec70a32692847a.
The result SHA256 is
1dacfa142b553a65e882bed2ba7b80fd5690a57791ef72878e965e1a954a3adb.

All 17 hosted workflows were green for measured commit bd4e43a8 before
submission, including CI run 34576383680, Security run 34576383752, and runtime
contracts run 34576383652. Local deterministic checks also passed before
the source commit. This single cell does not close the original criterion.
