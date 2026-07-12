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
eliminating remaining scanout error responses, long-duration graphics stress,
and integration into the normal app UX rather than the Windows HVF lab path.
