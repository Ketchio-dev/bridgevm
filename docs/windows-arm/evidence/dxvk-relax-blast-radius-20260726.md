# DXVK relax patch — blast radius and achievable feature level (2026-07-26)

What `scripts/patches/dxvk-macos-venus-relax.patch` actually costs. DirectX 11
works on this stack, but not as a conformant D3D11 implementation: five Vulkan
features DXVK treats as required are downgraded to reduced-caps so the Venus
adapter is accepted at all. Each one has a behavioural consequence, and they are
not all limited to exotic titles.

This document exists because the one-line framing — "we relax `geometryShader`"
— understates the change by four features. `depthClipEnable` in particular
affects **every** D3D title, not just geometry-shader ones.

## Achievable feature level

`D3D_FEATURE_LEVEL_11_0`, evidenced by the guest draw smoke
(`docs/history/windows-hvf/hvf-dxvk-d3d11-bringup-20260721.md`):

```text
BV-D3D11-DRAW-DEVICE feature_level=0xb000 mode=vb
BV-D3D11-DRAW-ADAPTER vendor=0x106b device=0x1a050209 desc=Virtio-GPU Venus (Apple M4 Max)
BV-D3D11-DRAW-RESULT center=ff00ffff magenta_pixels=4096 bad_pixels=0
BV-D3D11-DRAW-PASS
```

`0xb000` = `D3D_FEATURE_LEVEL_11_0`. Note the tension to keep in mind when
picking test titles: geometry shaders are a *mandatory* part of D3D10+, so a
D3D11 device that cannot run a GS is feature-level-11_0 by DXVK's reporting but
not by the D3D specification. Titles are therefore chosen against observed
behaviour, not against the advertised level.

D3D12 is out of scope for V1.

## The five relaxations

Line anchors are against DXVK tag **v3.0.2**, file `src/dxvk/dxvk_device_info.cpp`
(the `geometryShader` requirement is at :833 in that tag).

| Feature | Upstream | Patched | Consequence |
|---|---|---|---|
| `geometryShader` | required | `false` | Titles that use a geometry shader cannot run. Metal has no geometry stage, so this cannot be fixed in the guest. |
| `shaderCullDistance` | required | `false` | `SV_CullDistance` unavailable. Shaders relying on cull distance mis-render rather than fail loudly. |
| `depthClipEnable` | required | `false` | **Affects every title.** D3D semantics are depth *clip*; without `VK_EXT_depth_clip_enable` the pipeline depth-*clamps* instead. Geometry crossing the near/far plane is clamped into range rather than discarded — wrong pixels, not a crash. |
| `robustBufferAccess2` | required | `false` | Out-of-bounds buffer access is undefined instead of returning zero. A shader that reads past a binding gets garbage, and may fault. |
| `nullDescriptor` | required | `false` | **Reproduced defect** — see below. Draws with an unbound descriptor rasterize nothing. |

## Reproduced defect: null-descriptor draws render nothing

The same smoke, run twice, isolates this precisely. With a vertex buffer bound
the draw is pixel-perfect; with no vertex buffer bound (an `SV_VertexID`-driven
draw, which is legal D3D11 and common in fullscreen-pass code) it rasterizes
nothing:

```text
BV-D3D11-DRAW-DEVICE feature_level=0xb000 mode=vb
BV-D3D11-DRAW-RESULT center=ff00ffff magenta_pixels=4096 bad_pixels=0
BV-D3D11-DRAW-PASS

BV-D3D11-DRAW-DEVICE feature_level=0xb000 mode=novb
BV-D3D11-DRAW-RESULT center=000000ff magenta_pixels=0 bad_pixels=4096
BV-D3D11-DRAW-FAIL step=verify
```

Cause: DXVK's null-binding path assumes `nullDescriptor`, which the patch turned
off. Mitigation, not yet implemented: bind a dummy buffer in DXVK, or emulate
`nullDescriptor` in the stack. Real content mostly binds vertex buffers, which
is why the standard path works, but fullscreen passes are exactly the case that
does not — expect this to bite.

## What this means for title selection

Unsupported by construction:
- anything using geometry shaders;
- anything depending on `SV_CullDistance`.

Expect visual divergence, not failure:
- near/far-plane clipping behaviour in every title (depth clamp vs clip);
- shaders that read out of bounds and previously got zeros.

Expect missing geometry:
- fullscreen/vertex-ID passes with nothing bound, until the null-descriptor gap
  is mitigated.

## Reproduce

```
grep -n 'ENABLE_FEATURE\|ENABLE_EXT_FEATURE' scripts/patches/dxvk-macos-venus-relax.patch
git log --oneline -1 -- scripts/patches/dxvk-macos-venus-relax.patch
```

Live receipts:
- device + draw: `docs/history/windows-hvf/hvf-dxvk-d3d11-bringup-20260721.md`
- windowed present on the visible desktop:
  `~/BridgeVM/runs/venus-activate-120.40-demo3-20260721-060901/present-demo-visible-desktop.ppm`
