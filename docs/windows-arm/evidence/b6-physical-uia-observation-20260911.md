# B6 physical UIA coordinates: 2026-09-11 observations

This is historical diagnostic evidence, not a B6 criterion pass.
B6 remains OPEN: these runs do not establish the reviewed glyph masks,
three-resolution release matrix, or accepted frame-time baseline.

## Change and deterministic evidence

The native query now runs inside a caller-thread PER_MONITOR_AWARE_V2 scope.
The previous thread context is restored on both normal return and exception.
Button uniqueness, visibility, enabled state, process identity, owned-window
hit testing, display bounds, and the single-click limit remain enforced.
There is no guessed coordinate multiplier and no extra-click fallback.

The scoped query originated in ee00ebb6 and the asset-copy repair in 2249eed6.
The measured revision is f915b52f088bc514dbd9317f86f3105b65b98151.
Its 13 hosted workflows succeeded before the live jobs were submitted.
Windows contracts covered seven scope/restoration cases and five native
owned-button cases; actual asset-copy tests reject the old fused-path mutation.
These deterministic results do not replace the live observations below.

## Retained failures

At a044da7f, the 150% run returned owned point (415,172) four times.
The observed failure frame placed that pointer on popup explanatory text;
the actual button was near (624,258). The popup remained after one click.
Job: d3-b6-a044da7f-integer-negate-150-r1; capture-failed, zero completed runs.

At 25cca471, a fused quoted copy source stopped staging before the query ran.
Job: d3-b6-25cca471-physical-uia-150-r1; capture-failed, zero completed runs.
That experiment neither proved nor disproved the DPI-scope change.

The attempted 200% job d3-b6-f915b52f-physical-uia-200-r1 was rejected before
capture with input-failed: cell is outside the original B6 matrix.
The matrix was not broadened; its existing scales are 100%, 125%, and 150%.

## Completed single-resolution observations

All three cells used 1600x900 and the same sealed candidate renderer and probe.
Each owns its disk clone and vars; each receipt has three completed runs,
valid=true, outcome=observed, failure_code=none, and criterion_pass=false.
Diagnostic tracing is enabled, so these are not performance evidence.

| Scale | Job | Receipt SHA256 |
| --- | --- | --- |
| 100% | d3-b6-f915b52f-physical-uia-100-r1 | 00a0523dbea7428986950ea4f57144dc7f6ba6455310827f2112321f9699ebb2 |
| 125% | d3-b6-f915b52f-physical-uia-125-r1 | 7e9fe61161b898b21f4eea79dfc7aaed504f613c4f9d9710f9a267c661085649 |
| 150% | d3-b6-f915b52f-physical-uia-150-r1 | b80836dbd8ed7a59fb085237095b2dd8713bd23ce4d41b47ed1cb3bc16e6bdf8 |

Fresh native-query observations, in capture order:

100%:
```text
BVTIPPOINT hwnd=262346 state=present x=443 y=199 owner=262346
BVTIPPOINT hwnd=262346 state=not-found
BVTIPPOINT hwnd=328120 state=not-found
BVTIPPOINT hwnd=458876 state=not-found
```

125%:
```text
BVTIPPOINT hwnd=197192 state=present x=616 y=311 owner=197192
BVTIPPOINT hwnd=197192 state=not-found
BVTIPPOINT hwnd=262590 state=not-found
BVTIPPOINT hwnd=459342 state=not-found
```

150%:
```text
BVTIPPOINT hwnd=196796 state=present x=623 y=258 owner=196796
BVTIPPOINT hwnd=196796 state=not-found
BVTIPPOINT hwnd=197374 state=not-found
BVTIPPOINT hwnd=262912 state=not-found
```

The first popup was dismissed with one owned click in each cell; fresh queries
then reported not-found. The viewed first packaged frames at 125% and 150%
showed no popup and a 47-character body. This visual observation is not an
independent pixel oracle, complete color-correctness proof, or mask review.

Receipts and original frames remain in the private live-queue done directories
named above. The private physical-uia-150-comparison.json also retains the two
failed 150% experiments and their receipt hashes. Private guest assets are not
part of this document or git. No capability promotion follows from this record.
