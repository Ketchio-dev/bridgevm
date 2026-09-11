# Integer source negation: paired diagnostic observation

## Bounded conclusion

The TGSI translator negated the float carrier before converting its bits back
to a signed or unsigned integer. The correction negates the converted integer
instead. It is general source handling, not a glyph-layout fingerprint.
Float, untyped and double source handling are unchanged.

Six actual-translator fixtures cover unsigned temporary, signed temporary,
unsigned input, unsigned uniform-buffer, float and untyped sources. The four
integer fixtures failed against the old translator; all six passed against
the private candidate. Existing six sampler-typing fixtures also passed.
These tests inspect generated GLSL, not GPU execution or glyph correctness.

## Same-head physical observations

Both jobs used source `ed6cc8f8b02a7ba4ce8071ea8695ff8cc2271726`, whose 12
hosted workflows passed before submission. Image, initial vars, driver,
MoltenVK, cell configuration and trace policy hashes match between receipts.
Each job used its own private disk and vars clone.

| Observation | Job | Receipt SHA-256 |
| --- | --- | --- |
| Baseline | `d3-b6-ed6cc8f8-native-tip-control-r1` | `4c94213288010669250cd01c91177946e6001f1a796f9ea9f505a9202c52c312` |
| Candidate | `d3-b6-ed6cc8f8-integer-negate-r1` | `67b13d89d8f4b836a78c8588f2bbff6a2210725e914f81f6c25058ce0d95f86f` |

Both receipts are valid observations with three paired captures, no capture
failure, and false criterion/promotion flags. The native UIA path found an
owned Got it button, clicked it once, then returned a fresh not-found record.
The first packaged frame in each job visibly has no popup and contains body
text. Candidate text appears blacker than the fragmented cyan baseline.

The original diagnostic region remains x=75, y=144, width=360, height=16.
Neutral-dark means max(R,G,B)<96 and channel range<=12. Chromatic means
channel range>=80 and min(R,G,B)<180. Neither definition is a glyph mask.

| Run | Baseline neutral-dark | Candidate neutral-dark | Baseline chromatic | Candidate chromatic |
| --- | --- | --- | --- | --- |
| 1 | 59 | 246 | 849 | 1316 |
| 2 | 75 | 252 | 735 | 1144 |
| 3 | 79 | 247 | 795 | 1236 |

The chromatic count increased. It cannot be rewritten as a color-error
reduction, and visual blackness does not prove correct subpixel coverage.
The candidate trace contains integer negation after floatBitsToUint in the
8-minus-x expression; that establishes the changed shader was generated.

## Remaining requirements

B6 remains OPEN: this is one diagnostic cell, not the required 27-run matrix.
Reviewed glyph masks and an equivalent accepted frame-time baseline are absent.
Logging, assertions and warm-cache preparation remain confounders. No installed
renderer was replaced by these observations. The existing capability wording
and release state are unchanged. Raw guest captures remain private; receipts
and the private paired-body-ed6cc8f8.json report carry hashes and counts.
