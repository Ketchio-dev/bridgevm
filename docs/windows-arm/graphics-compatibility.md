# Windows ARM graphics compatibility

What the D3D11 and Vulkan paths actually do today, generated from a
registry so the table cannot drift from the code.

<!-- BEGIN GENERATED: graphics-compatibility -->
_Generated from `docs/windows-arm/graphics-compatibility.json` on 2026-08-04. Do not edit this block._

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
| PPSSPP | D3D11 | yes | Composed: Flip | 20.0 | 30 | **no** | 369 |
| bridgevm-d3d11-present-smoke (self-authored) | D3D11 | yes | Composed: Flip | 54.3 | 30 | yes | 311 |
| vkcube / Vulkan title | Vulkan | yes | unknown | not measured | 30 | **no** | 0 |

- **PPSSPP**: Best unrepeatable outlier 28.38 FPS. No Hardware: Independent Flip observed; DWM composition is the leading explanation and is not yet confirmed.
- **bridgevm-d3d11-present-smoke (self-authored)**: A smoke test, not a real title. It does not substitute for the A3 gate and is listed only to show the instrument works.
- **vkcube / Vulkan title**: Rendering is confirmed visually, but PresentMon observes no present events for this swapchain, so no frame rate has been measured. A guest Vulkan present instrument is still required.

<!-- END GENERATED: graphics-compatibility -->
