# Windows glyph text on macOS: integer vertex attributes

Status: body text fixed and live-proven. Title, tab and menu glyphs still do not
appear; the cause is narrowed and recorded here so the next attempt does not
repeat the disproved paths.

## The defect that was fixed

Apple's GL binds pure-integer vertex formats through `glVertexAttribIPointer`,
so the matching GLSL input must also be declared with an integer type. virgl
only did that under `VIRGL_USE_INTEGER`, so on macOS every pure-integer
attribute was declared as `float` and the driver reinterpreted the raw integer
bits as a float value. Windows 11 Notepad body text disappeared, because the
DirectWrite glyph pass feeds glyph data through `R16G16_SINT`/`R32_SINT`
attributes.

The fix scans the TGSI for shaders that provably consume an input as an
integer, tracking provenance through temporaries, and declares that shader's
pure-integer attributes as integers. See
`scripts/patches/virglrenderer-macos-venus.patch` and
`scripts/check-virgl-integer-attributes.sh`.

### Shapes that were tried and disproved

| Rule | Live result |
| --- | --- |
| Global `VIRGL_USE_INTEGER=1` | DWM regressed |
| Exact five-element DirectWrite layout signature | Worked, but is a fingerprint, not a rule |
| Shader-wide typing for any shader containing `UARL` | DWM regressed |
| Per-input typing: only inputs reaching `I2F`/`U2F`/`UARL` | Body text disappeared |
| Shader-wide typing once integer consumption is proven | Body text renders; shipped |

Per-input typing fails because the glyph vertex shader consumes three
attributes as integers and forwards the other two as raw bit patterns that the
paired fragment shader reads back with `floatBitsToUint`. Typing only the
converted inputs breaks the pass a second way.

## What remains, and what the live evidence proves about it

Title, tab and menu glyphs are still missing at a native 1280x1024 guest
scanout. Temporary instrumentation inside `vrend_draw_vbo` captured the first
glyph draw against each distinct render target during a single live Notepad
session. Seven distinct targets were captured. Every one of them showed:

- `ve->count == 5` with the expected `R16G16_SINT` x4 + `R32_SINT` layout,
- `uarl=0x0 i2f=0x7 u2f=0x0`, so provenance detection fires,
- `key signed=0x1f unsigned=0x0`, so all five attributes are declared as
  integers, which is the intended behaviour of the shipped rule,
- `compiled=1` and `gl_error=0`, so the program compiled and the draw executed.

The framebuffer readback taken immediately before and immediately after each
draw was effectively unchanged: for example the 948x598 Notepad target went
from `nonzero=32768` to `nonzero=32767` across the draw, and the 640x512 target
stayed at `nonzero=20143`.

This rules out the following as the cause of the remaining defect:

- integer attribute typing, which is now confirmed correct for these draws,
- shader compilation or link failure,
- GL errors at draw time,
- the vertex element layout, which is byte-identical to the body pass.

The remaining defect is therefore that these draws execute and write no pixels.
Note that the title pass uses an older DirectWrite fragment shader than the
body pass, keyed on an 8-pixel address expression rather than the body pass's
1023-mask expression, so the two consume the atlas differently even though they
share a vertex layout.

### Follow-up measurements

The readback above only covered the bottom-left 512x64 corner in GL
coordinates, which cannot see a title bar. Three further live sessions were run
to close that gap.

Hashing the *entire* colour attachment before and after each draw confirms the
split rather than removing it. Only the body pass changes its target:

| Context | Target | Atlas bound as `fssamp1` | Full-target hash across draw |
| --- | --- | --- | --- |
| 11 | 960x768 | 256x80 | unchanged |
| 21 | 640x512 | 256x80 | unchanged |
| 25 | 640x672, 960x640 | 256x80 | unchanged |
| 27 | 948x598 | 1024x20 | **changed** |

Viewport, scissor, colour mask, depth and stencil state were equivalent across
all of them, so none of those explain the split. The one structural difference
is the glyph atlas: the passes that render bind a 1024x20 atlas, and every pass
that renders nothing binds a 256x80 atlas.

An occlusion query around every glyph draw, rather than only the first per
target, shows fragments *are* rasterised by a failing pass: `ctx=11` at 960x768
passed 6131 samples across six draws while its target hash stayed identical.
So the failing draws are not rejected by scissor, depth or stencil; they shade
fragments whose result does not become visible.

Blending is not the reason. Forcing `glDisable(GL_BLEND)` around every glyph
draw makes the body text render as solid black boxes, proving the override took
effect, while the title, tab and menu text stays completely absent. That rules
out the blend state, and rules out the possibility that the missing text is
being drawn in a colour that blends away.

What remains is the sampled data itself. Dumping each distinct atlas shows the
256x80 atlases carry only about 2008 non-zero bytes out of 81920, and one
carries 992, so they are nearly empty.

## The fragments do reach the framebuffer

The "writes no pixels" reading above is wrong, and the measurement that
disproves it is decisive. Forcing `GL_INVERT` as a colour logic op around every
glyph draw makes all of the missing text appear at once: the window title, the
tab label, the File/Edit/View menu bar, the tooltip body, the status bar and
the taskbar search field. `GL_INVERT` ignores the fragment colour completely,
so the fragments were always landing on exactly the right pixels.

So the pass is not being discarded anywhere in the pipeline. The defect is the
colour the fragment shader computes.

Consistent with that, forcing blending off and diffing the colour target across
each draw reports `changed_px=0` for the failing passes. The shader is writing
the destination colour back unchanged, which is invisible whether or not
blending is enabled, and which is why every earlier framebuffer-difference
measurement read as "nothing happened".

## What the failing pass samples

The glyph fragment shader ends with `fsout_c0 = texelFetch(fssamp1, ...)`, so
whatever is bound as `fssamp1` becomes the output colour directly. Dumping that
texture for every context in one session separates the two groups cleanly.

The passes that render bind a 1024x20 texture whose row 0 holds per-channel
subpixel coverage, for example `00002500`, `00254a25`, `004a724a`. Channels
differ within a pixel, which is what ClearType coverage looks like.

Every failing pass binds a 256x80 texture with an entirely different character.
Only rows 0 and 1 contain data, all four channels of a pixel always hold the
same byte, there are only 17 distinct values, they repeat in groups of five,
and the row ends with a run of 256 pixels of solid `ffffffff`:

```
row0: 0 31 61 87 110  31 61 87 110 130  61 87 110 130 147  ...  255 x16
```

Both textures are glyph coverage, just laid out differently: the failing one
stores the same byte in all four channels, and the working one stores per
channel subpixel coverage. Neither is a wrong resource, so this is not a
sampler binding problem.

## The blend state differs, and that is where the colour is lost

Reading the real GL state at draw time separates the two groups exactly:

| | src factor | dst factor | blend colour | colour mask |
| --- | --- | --- | --- | --- |
| Failing (title, tab, menu) | `GL_ONE` | `GL_ONE_MINUS_SRC_ALPHA` | `(1,1,1,1)` | `0xf` |
| Working (body) | `GL_CONSTANT_COLOR` | `GL_ONE_MINUS_SRC_COLOR` | `(0,0,0,1)` | `0x7` |

The failing pass uses premultiplied alpha. Its shader writes the sampled
coverage value straight into all four channels, so a fully covered pixel
emits `(255,255,255,255)`, which under `GL_ONE, GL_ONE_MINUS_SRC_ALPHA`
replaces the destination with white. On white window chrome that is invisible,
and it is consistent with every measurement above: fragments arrive, the
target is written, and the written value equals what was already there.

A correction attempt that rewrote dual-source blend factors was built and
tested live, and it is recorded here only because it must not be tried again.
It was based on misreading the logged factor `0x303` as
`GL_ONE_MINUS_SRC1_COLOR`. `0x303` is `GL_ONE_MINUS_SRC_ALPHA`;
`GL_ONE_MINUS_SRC1_COLOR` is `0x88fa` and never appears in these traces. There
is no dual-source blending in this path, the fragment shaders correctly declare
a single output, and the change has been fully reverted. A cruder version of
the same experiment, which forced the substitution unconditionally, did make
desktop icon labels and the build watermark render, but it also removed the
taskbar and the Run dialog, so it regressed DWM and is not a viable direction.

### Everything else about the two passes is identical

One further live run compared the render target and sampler state of a failing
context against the working one. Every field matches:

```
ctx=11 atlas_w=256  surf_fmt=B8G8R8A8_UNORM res_fmt=B8G8R8A8_UNORM
                    swizzle=0x0 srgb=0x0 a8mask=0x0 fs_key_swz=0x0
                    atlas_view_fmt=B8G8R8A8_UNORM atlas_res_fmt=B8G8R8A8_UNORM
ctx=27 atlas_w=1024 surf_fmt=B8G8R8A8_UNORM res_fmt=B8G8R8A8_UNORM
                    swizzle=0x0 srgb=0x0 a8mask=0x0 fs_key_swz=0x0
                    atlas_view_fmt=B8G8R8A8_UNORM atlas_res_fmt=B8G8R8A8_UNORM
```

The two fragment shaders also end with byte-identical output statements,
`fsout_c0 = vec4((vec4(temp1)))` after the same `texelFetch` from `fssamp1`.

So the render target format, the resource format, the red/blue swizzle, the
sRGB encode mask, the A8 emulation mask, the shader-key swizzle and the sampled
texture format are all the same, and the only differences left between a pass
that renders and a pass that does not are ones the guest itself chose: the
blend factors, the blend colour, the colour mask, and the coverage layout in
the atlas.

That is the current boundary of this investigation. Nothing measured so far
shows virgl mistranslating the guest's request for the failing passes, so the
next step is to establish what the guest expects `GL_ONE,
GL_ONE_MINUS_SRC_ALPHA` with an all-channels-equal coverage value to produce,
rather than to change renderer state.

Raw dumps for all six contexts, the blend-state log, the target-state log and
the `GL_INVERT` proof are kept in `work/glyph-atlas-evidence/`.

## Correction (2026-08-16): the failing set was not one thing

Re-reading the retained dumps, without booting anything, contradicts two things
above. Both corrections come from files still in
`work/glyph-atlas-evidence/` and `/tmp/bv-glyph-{vs,fs}.glsl`.

**The glyph pass does not know the text colour.** The captured vertex shader
emits exactly two varyings, `vso_g1` (coordinates) and `vso_g2` (indices).
There is no colour varying, and the fragment shader ends with
`fsout_c0 = texelFetch(fssamp1, ...)`, so the pass writes the atlas value
itself. Whatever chooses the text colour is not in this draw.

**Most of the "failing" draws render to a 16x16 target.** `bv-g7-samplers.txt`
records the target for each captured context in that session:

| Context | Target | Atlas | Atlas pixel form |
| --- | --- | --- | --- |
| 11, 19, 21, 25 | **16x16** | 256x80 | `1f1f1f1f` - all four channels equal |
| 5 | 960x768 | 256x80 | `000000ff` - RGB zero, alpha only |
| 27 | 948x598 | 1024x20 | mostly per-channel, e.g. `00002500` |

A 16x16 render target is not window chrome. Those draws build an intermediate
glyph mask, and a mask that never reaches the screen is not a defect. Counting
them as failures inflated the problem.

The two atlas forms also mean different things. `(c,c,c,c)` is white text
premultiplied by coverage. `(0,0,0,a)` is black text premultiplied by coverage,
which is what window chrome should use.

The split is not absolute. In row 0 of the working 1024x20 atlas, 880 non-empty
pixels differ across channels and 143 do not, so "per-channel" describes most of
it rather than all of it.

**So `ctx=5` is the real anomaly, and the arithmetic says it should be
visible.** It draws to a 960x768 window target with a black-text atlas. Under
the recorded `GL_ONE, GL_ONE_MINUS_SRC_ALPHA` onto a white destination:

| atlas alpha | `0*1 + 1*(1-a)` |
| --- | --- |
| 0x3c | 0.765 |
| 0x58 | 0.655 |
| 0x76 | 0.537 |
| 0x96 | 0.412 |
| 0xff | 0.000 |

That is ordinary anti-aliased black text, not an invisible write. The earlier
section's conclusion -- that the shader writes the destination colour back
unchanged -- holds for the `(c,c,c,c)` atlases on a white destination, where
every one of the 16 distinct coverage values gives exactly 1.0. It does not
hold for `ctx=5`.

What is still unmeasured is `ctx=5` itself: its fragment shader was never
dumped, and neither was the destination it drew onto. Its atlas has 992
non-zero pixels against 2008 for the `(c,c,c,c)` group, so it is not the same
pass with a different atlas.

One hypothesis raised and killed in the same pass: that the working atlas is
distinguished by carrying zero alpha. Counting the retained dumps, the working
1024x20 atlas has only 15 fully transparent pixels out of 1023 non-empty ones,
so alpha presence does not separate the groups.

## Reproduction

The instrumentation is deliberately not in the tree. To repeat the measurement,
add a capture to `vrend_draw_vbo` keyed on `ve->count == 5 && info->count == 6`
and note two things that cost several boots to discover:

- these draws are indexed, so a predicate containing `!info->indexed` never
  fires,
- `render_log()` writes through `vsyslog` on macOS and is invisible in
  `run.log`; use `fprintf` to a file, with `setvbuf(..., _IONBF, ...)`.

Do not read `vrend_shader::glsl_strings` or call `glGetBufferSubData` on a
vertex buffer from that hook. Both segfaulted the probe.

When reading back a colour attachment, read the whole attachment. `glReadPixels`
origin is bottom-left, so a partial read from the origin covers the bottom of
the window and silently excludes the title bar.
