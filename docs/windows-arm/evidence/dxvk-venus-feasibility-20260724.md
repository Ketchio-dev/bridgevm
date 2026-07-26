# D0 feasibility receipt — DXVK-on-Venus DirectX→Metal path (2026-07-24)

> ## ⚠️ SUPERSEDED — the verdict below is wrong (corrected 2026-07-26)
>
> This document concluded `DX_METAL=BLOCKED_BY_HOST_CAPS` by auditing
> **unpatched upstream DXVK** against MoltenVK. It did not account for this
> repository's own relax patch, `scripts/patches/dxvk-macos-venus-relax.patch`,
> which had already been committed **three days earlier** and which turns
> `geometryShader` — the blocker named below — into a reduced-cap instead of a
> hard requirement.
>
> DirectX 11 on Venus was already working when this was written:
> commit `01405da` (2026-07-21) *"feat(3d): DXVK D3D11 reaches FL 11_0 device
> creation on Venus"*, with a live windowed present receipt at
> `~/BridgeVM/runs/venus-activate-120.40-demo3-20260721-060901/`
> (`present-demo-visible-desktop.ppm`, 1280×800, 60,600 magenta pixels: a DXVK
> D3D11 window composited by DWM on the Windows 11 desktop).
>
> **Authoritative status:** `docs/hvf-dxvk-d3d11-bringup-20260721.md`.
> **Relaxation consequences:** `docs/windows-arm/evidence/dxvk-relax-blast-radius-20260726.md`.
>
> The analysis below is retained because it remains a correct description of
> *unpatched* DXVK on MoltenVK, and it documents why the patch is necessary.
> Only its verdict and its impact-on-D3 section are void.

## Verdict (SUPERSEDED — see banner)

```
DX_METAL=BLOCKED_BY_HOST_CAPS   # WRONG: true only for unpatched DXVK
```

DXVK (both the current 3.0.2 and the older 1.10.3) hard-requires the Vulkan
`geometryShader` device feature. The host Vulkan implementation for this stack
is MoltenVK on Apple Silicon, which reports `geometryShader = false` because
Metal has no geometry-shader stage. The guest Venus ICD cannot expose a feature
the host physical device does not have, so no DXVK adapter can be created on the
BridgeVM viogpu3d/Venus path. Every other DXVK-required feature is available.

This is a host-capability wall (Metal/MoltenVK), not a DXVK build problem and
not a BridgeVM engine problem. It cannot be cleared without either a
geometry-shader-capable host Vulkan driver or a maintained DXVK fork that drops
the `geometryShader` requirement (out of scope — we do not fork DXVK).

## Part A — DXVK ARM64 build (VIABLE)

A pinned DXVK ARM64 Windows build succeeds with a zig-based llvm-mingw cross
toolchain, proving the artifact half of the path is real.

- DXVK tag: `v3.0.2` (`git describe --tags` in `~/BridgeVM/work/dxvk-build/src`)
- Toolchain: `zig 0.16.0` `cc`/`c++` targeting `aarch64-windows-gnu`;
  resources via `llvm-windres` (llvm@21) with the zig mingw
  `any-windows-any` include path.
- Cross file: `~/BridgeVM/work/dxvk-build/src/build-win-aarch64.txt`
  (`cpu_family = aarch64`, `system = windows`).
- Outputs (`PE32+ executable (DLL) Aarch64, for MS Windows`, `file`):
  - `d3d11.dll` — 4,983,808 bytes —
    SHA-256 `51922512f0ba797ed426113a22fa6b9731c10bfb8dbdeab6385f840c587e617d`
  - `dxgi.dll` — 3,497,984 bytes —
    SHA-256 `213344f080bbbabef0a03b8bbac5e2aaccf5284cd10f13250dbcffc4e9c9bddb`
- Build required recursive submodule init (`dxbc-spirv` needs `spirv_headers`).

## Part B — host/guest Vulkan capability audit (the blocker)

### Guest Venus ICD identity (from the preserved C8 clean run)
`wall-c8-clean-ppsspp-600s-20260723/guest-logs/bvgpu-vulkan-draw.log`:
```
device_name=Virtio-GPU Venus (Apple M4 Max) vendor=0x106b device=0x1a050209 api=0x40314e
```
`api=0x40314e` decodes to Vulkan **1.3.334** — satisfies DXVK's Vulkan 1.3
minimum. `bvgpu-vulkan-probe.log` records `enumerate_instance_version_result=0
api_version=0x0040312d` (1.3.301) with a live `vkCreateInstance` +
`vkEnumeratePhysicalDevices` success. The Venus device path is live.

### Host MoltenVK feature dump
`~/BridgeVM/runs/dx-probe-20260724/host-moltenvk-vulkaninfo.txt`
(`vulkaninfo`, MoltenVK 1.4.1, vulkan-loader 1.4.350.1):
- `deviceName = Apple M4 Max`, `apiVersion = 1.4.334`, `driverName = MoltenVK`

DXVK-required feature audit against the host physical device:

| DXVK-required feature (dxvk_device_info.cpp) | host MoltenVK |
|---|---|
| `geometryShader` | **false — BLOCKER** |
| tessellationShader | true |
| dualSrcBlend, multiViewport, imageCubeArray | true |
| shaderInt16, shaderInt64 | true |
| occlusionQueryPrecise, fragmentStoresAndAtomics | true |
| vk12 vulkanMemoryModel, timelineSemaphore, bufferDeviceAddress | true |
| vk12 descriptorIndexing, scalarBlockLayout | true |
| vk11 shaderDrawParameters, storageBuffer16BitAccess | true |
| vk13 dynamicRendering, synchronization2, maintenance4 | true |

### DXVK hard requirement (both versions)
- 3.0.2: `src/dxvk/dxvk_device_info.cpp:833`
  `ENABLE_FEATURE(core.features, geometryShader, true)` → adapter rejected with
  "Device does not support required feature 'geometryShader'".
- 1.10.3: `src/d3d11/d3d11_device.cpp:1933`
  `enabled.core.features.geometryShader = VK_TRUE` (unconditional), checked in
  `dxvk_adapter.cpp:121`.

## Impact on D3 (VOID — see banner)

~~D3's DirectX→Metal render receipt is redefined to its `BLOCKED_BY_HOST_CAPS`
branch.~~ This conclusion is withdrawn. DirectX→Venus→Metal has a positive live
receipt predating this document; D3 lands on its **positive** branch, not the
negative boundary. What is genuinely walled is a *fully conformant* DXVK: the
relaxed features carry real behavioural consequences, enumerated in
`dxvk-relax-blast-radius-20260726.md`.

## Reproduce

```
# Host feature (authoritative blocker):
VK_ICD_FILENAMES=/opt/homebrew/etc/vulkan/icd.d/MoltenVK_icd.json \
  vulkaninfo | grep -E '^\s*geometryShader '
# -> geometryShader = false

# DXVK requirement:
grep -n 'geometryShader' ~/BridgeVM/work/dxvk-build/src/src/dxvk/dxvk_device_info.cpp
# -> 833: ENABLE_FEATURE(core.features, geometryShader, true)
```
