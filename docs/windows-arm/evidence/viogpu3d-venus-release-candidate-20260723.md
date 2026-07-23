# Windows ARM64 Venus GPU release-candidate receipt (2026-07-23)

## Result

`GPU-WDK-SIGN` is submission-ready up to the explicitly excluded production
EV/Partner Center signature. The latest ARM64 Venus package is catalogued,
test-signed, render-candidate complete, cleanly bound in Windows, and passes the
host/device/readiness gates.

This receipt does not claim production signing and does not replace the separate
same-boot PPSSPP 10-minute live-title gate.

## Package

Path: `~/BridgeVM/work/download-120.43-fence-revert/`

Provenance:

```text
source_repo=anonymix007/kvm-guest-drivers-windows-venus + arehnman/virtio-win-mesa
source_ref=viogpu3d-venus-wip + mesa@cb531c440ff34a9c6334859dda0848132be49ec3
build_id=29864055824-64
github_actions_run=29864055824
signing_cert=self-signed BridgeVM viogpu3d Test Signing
```

Contents include ARM64 `viogpu3d.sys`, catalog/INF/certificate,
`viogpu_d3d10.dll`, `vulkan_virtio.dll`, and `virtio_icd.arm64.json`.

## Deterministic package gate

Evidence: `~/BridgeVM/runs/c7-package-check-20260723/`

`check-hvf-windows-viogpu3d-package.sh --require-render-candidate` reports:

```text
expected_hwid=PCI\\VEN_1AF4&DEV_1050
protocol=venus
package_capability=umd-registered
render_candidate=true
umd_registration=complete
umd_user_mode_driver_name_registered=true
umd_installed_display_drivers_registered=true
umd_vulkan_driver_name_registered=true
umd_registered_dlls_resolved=true
umd_active_copyfiles_payload_resolved=true
PASS: viogpu3d package is injection-ready
```

## Host/readiness gate

Evidence: `~/BridgeVM/runs/c7-readiness-20260723/readiness.txt`

```text
host_preflight=PASS
host_expected_capset_id=4
host_expected_capset_ok=true
host_renderer_venus=AVAILABLE
host_renderer_venus_available=true
host_backend_venus_runtime=WIRED
boot_ready=true
PASS: P3 Windows GPU readiness
```

## Live Windows bind and Vulkan proof

Live evidence:
`~/BridgeVM/runs/venus-activate-120.43-grand10-20260721-165317/`

The firstboot receipt reports:

```text
Status=OK
Class=Display
FriendlyName=Hardsoft VirtIO GPU 3D controller (venus)
DriverVersion=120.43.0.0
InfName=oem43.inf
expected_inf_sha256=2FAD4BA828CD0442A0F20E63CC9DF5381C8F5EE0965F80E028F0F8A5B0816961
bound_inf_sha256=2FAD4BA828CD0442A0F20E63CC9DF5381C8F5EE0965F80E028F0F8A5B0816961
```

The same boot proves the ARM64 Vulkan path:

```text
create_instance_result=0
enumerate_physical_devices_result=0 count=1
device_name=Virtio-GPU Venus (Apple M4 Max)
gate_clear=PASS
gate_draw=PASS
gate_bench=PASS
bench frames=300 fps=4177.57
```

The host trace report selected Venus and passed the P3 gate with accepted
features, capset 4, non-empty submit, parked fence, fence completion/delivery,
and 474 scanout readbacks:

```text
P3 Windows 3D trace gate: PASS
Blockers: none
```

## Boundary

Production EV/Partner Center signing remains external and out of scope. C8
still requires a packaged-app, same-boot PPSSPP Vulkan title receipt lasting at
least ten minutes plus framebuffer-rate metrics.
