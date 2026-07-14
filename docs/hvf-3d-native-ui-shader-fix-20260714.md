# Windows native-UI render bug — root-caused and fixed (2026-07-14)

## Symptom
In the live Windows guest, browser content rendered fine but **Windows
native UI (Settings, taskbar, DWM/DirectWrite text) rendered wrong**. This
is distinct from the display's low frame rate (a separate 500 ms
screenshot-pipeline issue).

## Root cause (captured live)
Booting the installed disk with `VIRGL_LOG_LEVEL=debug VREND_DEBUG=shader`
and opening Settings deterministically reproduced a host shader-compile
failure:

```
vrend_compile_shader: context error reported 20 "" Illegal shader 0
Shader failed to compile
ERROR: 0:4: '' :  extension 'GL_ARB_shader_draw_parameters' is not supported
VERT ... DCL SV[0], VERTEXID_NOBASE
```

The failing shader is a **vertex shader** declaring TGSI `VERTEXID_NOBASE`
(D3D `SV_VertexID`). vrend's `sysvalue_map` maps `VERTEXID_NOBASE` to the
GLSL string `(gl_VertexID - gl_BaseVertexARB)` and **unconditionally**
requires `SHADER_REQ_SHADER_DRAW_PARAMETERS` → emits
`#extension GL_ARB_shader_draw_parameters`. **Apple OpenGL 4.1 does not have
that extension** (`feat_draw_parameters` is UNAVAIL on this host), so the
GLSL fails to compile, `DxgkDdiRender` draws for that context fail
(`DRAW_VBO` error), and the native UI drawn by it is garbled. Browsers use
shaders that never touch this system value, so they render correctly — which
is exactly the observed split.

This is the concrete instance of the "intermittent vrend shader-translation
hole" flagged in `docs/hvf-3d-perf-baseline-20260714.md`; it is
state-dependent (idle desktop does not trigger it; opening native
DirectWrite-heavy UI does).

## The fix (3 files in virglrenderer, in the macos-venus patch)
Give the shader translator the missing capability bit and fall back cleanly
when the extension is unavailable:

1. `vrend_shader.h` — add `has_draw_parameters : 1` to `struct vrend_shader_cfg`.
2. `vrend_renderer.c` — populate it: `shader_cfg.has_draw_parameters = has_feature(feat_draw_parameters)`.
3. `vrend_shader.c` (`iter_declaration`, system-value handler) — when the
   declared system value is `VERTEXID_NOBASE` and `!cfg->has_draw_parameters`,
   emit plain `gl_VertexID` and drop `SHADER_REQ_SHADER_DRAW_PARAMETERS` from
   the shader's required extensions.

`VERTEXID_NOBASE` = `SV_VertexID` without the base-vertex offset; without the
extension the base vertex cannot be subtracted, so `gl_VertexID` (base = 0)
is the correct value for non-BaseVertex draws — which is what `SV_VertexID`
fullscreen-triangle / instanced UI draws use. Only `VERTEXID_NOBASE` is
special-cased; `BASEVERTEX`/`BASEINSTANCE`/`DRAWID` are left unchanged (not
observed in the failing native-UI path).

Implemented by gpt-5.6-sol (medium) per delegation policy; diagnosis and
capture done here.

## Verification (minimal, one boot)
Rebuilt `libvirglrenderer`, booted the installed disk, opened Settings +
Notepad + the "This PC" shell (the exact triggers that reproduced the
failure). Result: **zero** `Shader failed to compile` /
`GL_ARB_shader_draw_parameters` / `Illegal shader` lines
(`~/… /bridgevm-shaderfix-verify/run.log`). The only remaining renderer
diagnostic is the unrelated, pre-existing dwm `context 6 … DRAW_VBO: 22`
("Illegal command buffer") seen on every boot.

Fix carried in `scripts/patches/virglrenderer-macos-venus.patch`
(regenerated). The separate 2 FPS / blocky-text display-pipeline issue
(500 ms PPM export + `.interpolation(.none)`) is not addressed here.
