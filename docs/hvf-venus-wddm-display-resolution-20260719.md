# Windows Venus WDDM display-start resolution (2026-07-19)

## Status

Resolved. Windows now advances from display discovery into the WDDM D3D render
path, submits non-bootstrap 3D command streams, and presents a visible Windows
desktop through the active virtio-gpu scanout without a BugCheck.

The same bounded run also completes the guest Vulkan physical-device gate and
delivers guest Venus commands to the host backend configured with MoltenVK.

## Diagnosis

The original display-response theory was eliminated by protocol evidence. Both
`GET_DISPLAY_INFO` and `GET_EDID` returned complete, correctly sized response
buffers with valid headers, fence/ring fields, scanout 0 geometry, and a valid
128-byte EDID checksum. The guest also continued with `RESOURCE_CREATE_3D` and
`SET_SCANOUT`, so KMD `StartDevice` was not stopping after the queries.

Driver 120.34 added narrow Mesa D3D beacons around device/context creation,
draw, present, flush, and `pfnRenderCb`. It showed:

- successful D3D device and context creation
- completed draw, DXGI present, and D3DKMT present callbacks
- populated command, allocation, and patch lists
- 227 of 227 render callbacks returning `0x88760879`
  (`D3DDDIERR_INVALIDUSERBUFFER`)
- no corresponding D3D command reaching host virtio-gpu

The rejected buffer had an exact shared-ABI mismatch. The Venus KMD parses a
16-byte `VIOGPU_COMMAND_HDR` containing `type`, `size`, `flags`, and `ring_idx`.
Pinned Mesa exposed only the first two fields, so the UMD submitted
`CommandLength = 8 + payload`. The KMD advanced past 16 bytes and observed a
body exactly eight bytes shorter than the advertised payload size.

## Fix

Builder commit `bc5a2edef954168e3deacac2d193d4abc3480b9c` adds the missing
`flags` and `ring_idx` fields to Mesa's WDDM header and zero-initializes the
complete header at every D3D and D3DKMT producer. It also advances the package
version to `120.35.0.0`.

The patch was applied with the complete pinned Mesa patch chain before CI. The
Windows ARM64 build then succeeded as GitHub Actions run `29698757998` and
produced package build id `29698757998-54`.

Related device-model commits on this branch are:

- `ee36703`: record descriptor response capacity in GPU traces
- `8f37894`: read back native 3D scanout dimensions

## Verification

The package passed the render-candidate checker and was injected only into the
development RAW clone. Injection stopped through clean PSCI `SYSTEM_OFF` with
235 successful NSID-2 writes and 15 successful flushes.

Activation evidence is stored at:

```text
/Users/insighton/BridgeVM/runs/venus-activate-120.35-command-header-20260719-144347
```

The installed `oem35.inf` reports `120.35.0.0`. Offline SHA-256 values for the
installed KMD, D3D UMD, and Vulkan ICD exactly match the downloaded artifact.

The appended guest beacon log retains the old driver calls made before live
replacement, then records a decisive status transition: 305 old
`0x88760879` results are followed by 203 consecutive `S_OK` results. The final
clean 120.35 boot segment has 104 of 104 successful render callbacks and no D3D
`SetError` beacon.

Host trace results are:

- 483 successful `SUBMIT_3D` commands and zero submit response errors
- 477 non-bootstrap submits, ranging from 16 to 130,036 bytes
- 1,374 `RESOURCE_CREATE_3D` commands
- 572 `SET_SCANOUT` commands
- 562 successful scanout readbacks
- P3 Windows 3D trace gate PASS with no blocker

The active WDDM path cycles 1024x768 resources on the 1280x800 host canvas. The
final frame contains 786,349 nonzero pixels and 25,582 unique colors, compared
with only 15 nonzero pixels on 120.34. The preserved frame visibly contains the
Windows 11 desktop, taskbar, icons, and Terminal window. Its checksum is
`0x7f6f0a592c95a574`.

Two expected installation reboots completed, the final boot remained live for
the 120-second bounded observation, and no BugCheck signature or Windows dump
was created.

## Vulkan and host continuation

The guest probe exits with code 12 when `vkEnumeratePhysicalDevices` returns
fewer than one device. Its persistent firstboot task deletes itself only after
that probe returns zero and the exact bound-INF check passes. That task is
absent after this run, closing the `>= 1` control-flow gate.

The final trace then shows a Venus `context_init=4` context (`ctx29`) and a
`virgl-shadow-win32` context. The Venus context sends 21 `SUBMIT_3D` commands;
all return `OK_NODATA`. The run preflight pins
`/opt/homebrew/lib/libMoltenVK.dylib` and the trace reports the 3D backend
attached, proving that the guest Venus stream crossed into the MoltenVK-backed
host renderer.

The human-readable Vulkan probe log still has a same-file dual-writer race
between the probe and its wrapper, so this run intentionally relies on the
probe's enforced exit gate and the host command trace instead of quoting a
possibly clobbered count line.

## Remaining work

The display-start wall is closed. The next focused work is a dedicated guest
Vulkan draw/present workload with an image-level assertion, followed by cleanup
of the temporary D3D beacons and the probe-log writer race. Repeated
`RESOURCE_UNMAP_BLOB` invalid-parameter responses also remain visible and should
be separated into already-unmapped cleanup versus a real mapping-lifecycle bug.
