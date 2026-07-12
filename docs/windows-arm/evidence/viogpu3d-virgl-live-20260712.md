# Live Windows ARM64 VirGL/WDDM evidence — 2026-07-12

BridgeVM's no-QEMU HVF engine completed a live Windows 11 ARM64 boot with the
test-signed `viogpu3d` full package bound to virtio-gpu `DEV_1050` and the
macOS CGL-backed VirGL renderer active.

The preserved evidence store is
`/Users/user/BridgeVM/viogpu3d-dev1050-legacy-virgl-proof-20260712-v15`.
It contains the run log, service/readiness gates, and a 23,421-event
`virtio-gpu.jsonl` trace. The guest reported:

- the Hardsoft VirtIO GPU 3D controller present with status `OK`, problem code
  zero, `oem4.inf`, driver 1.1.1.2, and the `VioGpu3D` service running;
- `viogpu_d3d10.dll`, a 1280x800 60 Hz mode, DDI 11.2, feature levels through
  10_0, and WDDM 1.3 through `dxdiag`;
- successful resident-agent commands followed by exit-0 `shutdown.exe /p /f`.

The host observed VIRGL/VIRGL2 capsets 1/2, successful
`RESOURCE_CREATE_3D`, `RESOURCE_ATTACH_BACKING`, protocol-matched
`CTX_CREATE`, non-empty `SUBMIT_3D`, and the complete renderer-fence lifecycle.
The protocol-specific P3 trace gate passes with no blockers and no invalid
JSONL events. PSCI system-off and final disk/UEFI-vars writeback completed.

The macOS renderer fix unbinds buffer textures around Apple OpenGL buffer
mutation, stages buffer transfers on the CPU where required, and serializes
CGL-context submission. This avoids the Apple driver crash previously reached
in `gleUpdateCtxDirtyStateForBufStampChange` while preserving exact texture
bindings afterward.

This closes the earlier “driver package / host VirGL / live binding” wall. It
does not turn the test-signed driver into a production-distributable package,
and the observed feature ceiling is D3D feature level 10_0 rather than 11/12.
The next wall is productization: repeatable package provenance/signing,
long-duration graphics stress, and integration into the normal app UX rather
than the Windows HVF lab path.

## 3D scanout closure

The follow-up v17 evidence store is
`/Users/user/BridgeVM/viogpu3d-dev1050-legacy-virgl-proof-20260712-v17`.
BridgeVM now accepts a VirGL `RESOURCE_CREATE_3D` resource in `SET_SCANOUT` and
reads the renderer texture into the app-owned XRGB8888 framebuffer on
`RESOURCE_FLUSH`. The preserved 60-second 1280x800 image shows the Windows 11
desktop, taskbar, wallpaper, and icons rather than the all-black v15 output.
Its PNG conversion has SHA-256
`b0028d85b7959c2a845422c77b1ebedc2793349bcfc5a4a67597f808b9d54bbe`.

The v17 run observed 429 successful `SET_SCANOUT` commands and zero scanout
error responses. The Apple path directly uses the successful
`glGetTexImage` readback rather than first issuing a predictably rejected
BGRA framebuffer read. Consequently the run contains zero
`glReadPixels failed` messages. Its 15,652-event required VirGL trace gate,
agent-service gate, PSCI system-off, NVMe writeback, and cleanup all completed
with status zero.

The final v18 compatibility run is preserved at
`/Users/user/BridgeVM/viogpu3d-dev1050-legacy-virgl-proof-20260712-v18`.
It matches QEMU's legacy VirGL wire behavior by retaining five renderer-side
submit diagnostics in the host log without converting them into guest-visible
`SUBMIT_3D` failures. Across 11,846 trace events there were zero virtio-gpu
error responses, the required trace and resident-agent gates both returned
status zero, and the 30-second desktop PNG has SHA-256
`bd2405652d121ec9e088363810927f1f97196b3e9600d6dfd6cf8ab454078575`.
The agent completed an exit-zero guest shutdown and the gate confirmed PSCI
system-off and NVMe writeback.

## Normal app live-display closure

The v19 app-path run is preserved at
`/Users/user/BridgeVM/app-virgl-live-display-proof-20260712-v19`. The normal
macOS HVF configuration now launches Windows with VirGL, PCI device ID 1050,
and buffered NVMe enabled by default. The runtime atomically replaces one
bounded `display.ppm` artifact every 500 ms, and the app's Live Display view
decodes that file before falling back to diagnostic RAMFB checkpoints.

During the live Windows run the 3,072,016-byte 1280x800 artifact changed both
modification time and SHA-256 across consecutive observations. The final frame
captured the Windows shutdown spinner after the app-equivalent control command,
showing that export continued through guest shutdown. The final PPM SHA-256 is
`b818efec7a2f6d871d819cddf8759194af56f9fbcdfcb452ce1dbafda7213ff5`.
The run recorded zero virtio-gpu error responses; its VirGL trace gate,
resident-agent gate, PSCI system-off, NVMe writeback, and cleanup all returned
status zero.

The paced v21 follow-up is preserved at
`/Users/user/BridgeVM/app-virgl-paced-display-proof-20260712-v21`. Windows uses
triple-buffered scanout resources, so the pacing clock deliberately survives
`SET_SCANOUT` resource rotation. With the app's 500 ms display interval, the
run coalesced 432 guest `RESOURCE_FLUSH` commands into 25 renderer readbacks,
eliminating 94.21% of full 1280x800 GPU-to-CPU copies. The bounded live frame
continued updating through the Windows shutdown spinner and ended with PPM
SHA-256 `fe53e51bb2c556c84034403bb911b25bb9e2a1a32c181eba6b9315b749a9140d`.
There were zero virtio-gpu error responses, and the agent, VirGL trace, PSCI
system-off, NVMe writeback, and cleanup gates all returned status zero.

## Normal app live-input closure

The v23 app-path run is preserved at
`/Users/user/BridgeVM/app-live-input-proof-20260712-v23`. The normal macOS app
now creates an append-only input control channel, enables the emulated xHCI HID
devices, and maps aspect-fit display clicks into the guest's 0...32767 absolute
pointer coordinate space. It also exposes bounded text, Tab, Enter, and Space
keyboard controls while reusing the same validated HID action grammar as the
installed-boot probe.

The before-click Windows desktop frame has SHA-256
`e0db758b2616949d755ea70b1db4f2ac6ec243c621278d53f266c0cbad80713e`.
After the runtime accepted `POINTER click:20966x31783`, the next frame has
SHA-256 `3488969b99293c09d53c9c7def46a2e6671327cfb9bb13feb1bf70dd77beba20`
and visibly shows the Microsoft Edge first-run window opened from the taskbar.
This proves the complete app-control-file, host poller, xHCI queue, guest HID,
and live-display feedback loop rather than only parser or device-level input.

The run recorded zero virtio-gpu error responses. Its resident-agent and VirGL
trace gates, guest system-off, NVMe writeback, and host cleanup all returned
status zero.

## Relocatable app-bundle closure

The v24 packaged-runtime run is preserved at
`/Users/user/BridgeVM/app-bundled-runtime-proof-20260712-v24`. The macOS bundle
now builds the installed-Windows probe with renderer support enabled, embeds
`libvirglrenderer.1.dylib` and `libepoxy.0.dylib` in the nested HVF app, rewrites
all development-host install names to `@loader_path`, and signs and verifies the
complete nested runtime. A loader probe confirmed that both libraries were
resolved from `BridgeVMControl.app/Contents/Frameworks` with no `/Users` or
Homebrew dependency path.

The live boot was launched exclusively through the wrapper and probe copied
inside that app bundle. It reached `BVAGENT READY`, accepted
`POINTER click:13619x32357`, and changed the exported display from SHA-256
`445e0c3d549d28e51705c376379802af02df6c600a36ee1970f18018dff62f75` to
`b9a94a538b0f1ff0fff9f18f3f732550d0a6aa20c0284441d682ffe8ad7407f1`.
The resident agent then completed an exit-zero shutdown. Guest system-off,
UEFI-vars and NVMe writeback, and host cleanup all returned status zero, with no
dynamic-loader, panic, or virtio-gpu error in the run log.

## Bundled firmware closure

The v25 packaged-runtime run is preserved at
`/Users/user/BridgeVM/app-bundled-firmware-proof-20260712-v25`. The nested HVF
app now carries the 64 MiB AArch64 EDK2 code image, its upstream license
notices, and a size/SHA-256 manifest. The installed-boot wrapper accepts an
explicit firmware path, otherwise resolves the bundle before standard QEMU
install locations, validates the exact pflash size before launch, and records
the selected path in preflight evidence.

This run deliberately omitted `--firmware-code`. Preflight selected
`BridgeVMControl.app/Contents/Resources/firmware/edk2-aarch64-code.fd`, whose
SHA-256 is `47765fe344818cbc464b1c14ae658fb4b854f5c2ceffa982411731eb4865594d`.
That bundle-owned firmware reached the Windows desktop and `BVAGENT READY` with
VirGL enabled; the exported display SHA-256 is
`406813e784bb1b182c6fabf3dedc5fd8ae62e1972b798bd3b1ada03f9572f979`.
The agent gate, guest system-off, UEFI-vars and NVMe writeback, and cleanup all
returned status zero, with no loader, panic, or virtio-gpu error.
