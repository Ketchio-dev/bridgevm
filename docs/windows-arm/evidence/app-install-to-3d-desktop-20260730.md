# Standalone app install to 3D desktop — 2026-07-30

## Scope

This receipt closes A9 for the BridgeVM V1 app path: user-provided Windows 11
ARM64 ISO → scripted installation → viogpu3d injection → Windows desktop on the
3D scanout. All builders and boot wrappers used below came from the packaged
`BridgeVMControl.app` resource root, not the repository.

## Packaged-resource closure

Package under test: `/tmp/BridgeVMControl-A9-v3.app`.

- `codesign --verify --deep --strict` passed.
- The bundle contains the scripted-source builder, injector builders/checker,
  scripted installer, signed release probe, mandatory WinPE/firstboot/agent
  assets, and both Vulkan draw-smoke source/header files.
- The install command now passes `--release --skip-build`, so a standalone app
  reuses its bundled signed probe instead of attempting a Cargo build.

## Live chain

1. The bundle-only source builder consumed the user ISO and produced
   `/tmp/bridgevm-a9-bundled-source.raw` (16 GiB sparse), SHA-256
   `c59ae6f10a92bb56766e0a3c22bf0c870eee0b2abc5af61393e357742ff0c160`.
   Its WIM verification found `winpeshl.ini`, `bvinstall.cmd`, and
   `bvdiskpart.txt` and verified `EFI/BOOT/BOOTAA64.EFI` plus split install SWMs.
2. The bundle-only injector builder validated the working 120.41 Venus package
   and produced `/tmp/bridgevm-a9-bundled-injector-v3.raw`, SHA-256
   `f0dd0515e6c806b56cb78aa6c52b4a1441c47c0f3d3a9694a865ef5c8f2efc88`.
   It built/staged the ARM64 agent, diagnostics service, and Vulkan draw smoke.
3. The bundle-only scripted installer created a fresh 64 GiB target and exited
   0 via PSCI `SYSTEM_OFF`. Physical inspection found GPT partitions EFI,
   Microsoft Reserved, and NTFS `Windows`. Evidence reported 176 successful
   NSID-2 writes and 15 flushes with `target_effect_class=present_successful_io_write`.
4. The injector boot recovered injector-only `bvagent-package.log`, performed
   199 successful NSID-2 writes plus 33 flushes, and exited through PSCI
   `SYSTEM_OFF` with no owned process left behind.
5. The initial implementation exposed a real ordering bug: viogpu3d stage 1
   rebooted during Windows specialize, producing the Windows “installation
   cannot proceed” dialog. Offline Panther logs proved the unattended file was
   consumed and setup was still in `IMAGE_STATE_UNDEPLOYABLE`.
6. The fixed firstboot script defers activation while `ImageState` is not
   `IMAGE_STATE_COMPLETE`. The final run logged two `setup-deferred` entries,
   completed OOBE/autologon, then advanced stages 1–4 across fresh boot
   identities without interrupting setup.

## Final 3D receipt

Evidence directory: `/tmp/bridgevm-a9-v3-work/firstboot-evidence`.

- `firstboot-stage.txt`: `last_stage_observed=stage4`, `stage4_pass=1`.
- Driver: `Status=OK`, version `120.41.0.0`, bound `oem1.inf`.
- Expected and bound INF SHA-256 both equal
  `2CD1735D0E0B79F42CC75FE8479773D198C06142862875C2C6FAD3D9C7A45C40`.
- `BVGPU-DRIVER-STATE-PASS`; Vulkan probe and Vulkan draw smoke both returned 0.
- Guest 3D commands: `RESOURCE_CREATE_3D=514`, `CTX_CREATE=26`,
  `SUBMIT_3D=821`, `RESOURCE_FLUSH=237`, `SET_SCANOUT=231`.
- Final 1280×800 artifact visibly shows the Windows 11 desktop:
  `app-install-3d-desktop-20260730.png`, SHA-256
  `9d3629217079895ab7642b55b0e69f51693db57b868453adcc1770dadf92ac0d`.
- Cleanup receipt reports status 0 and no remaining owned probe/wrapper process.

This proves the app-generated fresh install can reach a real 3D Windows desktop.
It does not substitute for the separate A2/A3 real-title FPS criteria.
