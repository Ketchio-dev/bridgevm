# Windows ARM graphics compatibility

What the D3D11 and Vulkan paths actually do today, generated from a
registry so the table cannot drift from the code.

<!-- BEGIN GENERATED: graphics-compatibility -->
_Generated from `docs/windows-arm/graphics-compatibility.json` on 2026-09-01. Do not edit this block._

## Stack

| Layer | Component |
| --- | --- |
| Guest API | D3D11 via DXVK |
| Translation | DXVK (patched) |
| Guest driver | viogpu3d Venus ARM64 |
| Host backend | virglrenderer Venus over Metal |

Driver provenance is keyed by DriverStore hash, because a rebuilt package with the same file name is a different driver:

- fixed: `viogpu3d.inf_arm64_6435ce2e01767d8f`
- shipped 120.41: `viogpu3d.inf_arm64_44e90b7a44a1d335`

## Conformance

**Experimental D3D11-compatible subset.** Five Vulkan features DXVK requests for feature level 11_0 are disabled. Advertising full FL11_0 conformance while those are relaxed would be false.

## Relaxed features

DXVK requests these for feature level 11_0. They are not provided, and each has a guest-visible consequence:

| Feature | Layer | Consequence |
| --- | --- | --- |
| `geometryShader` | vulkan core | A title that needs a geometry stage will fail to create its device or render incorrectly. |
| `shaderCullDistance` | vulkan core | Shaders using cull distance lose that output; geometry that should be culled may be drawn. |
| `depthClipEnable` | VK_EXT_depth_clip_enable | Depth clipping falls back to clamping semantics; geometry outside the depth range can be drawn where D3D11 would clip it. |
| `robustBufferAccess2` | VK_EXT_robustness2 | Out-of-bounds buffer reads are not guaranteed to return zero, so a shader bug can read adjacent data instead of a defined value. |
| `nullDescriptor` | VK_EXT_robustness2 | Binding a null descriptor is undefined rather than reading as zero; DXVK relies on this for unbound D3D11 resources. |

## Titles

| Title | API | Renders | Present mode | p50 FPS | Gate | Meets gate | Samples |
| --- | --- | --- | --- | --- | --- | --- | --- |
| PPSSPP | D3D11 | yes | Composed: Flip | 62.5 | 30 | yes | 13277 |
| bridgevm-d3d11-present-smoke (self-authored) | D3D11 | yes | Composed: Flip | 54.3 | 30 | yes | 311 |
| PPSSPP | Vulkan | yes | not measured | 58.8 | 30 | yes | 7570 |

- **PPSSPP**: The fixed three-run live campaign passed 3/3. Per-run sample counts were 2633, 5275 and 5369; p50 values were 250.0, 62.5 and 62.5 FPS. Composed: Flip was observed in the earlier retained D3D11 title run; the campaign did not relax its 30 FPS threshold or substitute the self-authored smoke.
- **bridgevm-d3d11-present-smoke (self-authored)**: A smoke test, not a real title. It does not substitute for the A3 gate and is listed only to show the instrument works.
- **PPSSPP**: Three retained runs passed with 2511, 2528 and 2531 samples; each reported p50 58.82 FPS. The title's own guest frame intervals are used because this PPSSPP build emits no fps log lines and PresentMon does not observe this Vulkan swapchain.

<!-- END GENERATED: graphics-compatibility -->
