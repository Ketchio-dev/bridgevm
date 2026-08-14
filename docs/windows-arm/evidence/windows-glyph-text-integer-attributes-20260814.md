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
carries 992, so they are nearly empty. The next step is to establish whether
those atlases are ever populated with glyph coverage, by tracing the transfers
and blits that write the texture backing `fssamp1` for a failing context, and
comparing them against the 1024x20 atlas the body pass uses.

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
