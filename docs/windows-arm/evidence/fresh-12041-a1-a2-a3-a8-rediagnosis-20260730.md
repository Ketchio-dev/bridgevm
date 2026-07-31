# Fresh WDDM 120.41 A1/A2/A3/A8 re-diagnosis — 2026-07-30

## Result

The fresh A9 image changed several failure classifications, but it did **not** pass A1, A2, A3, or A8. Shipping criteria remain unchanged.

A preserved source pair was created before testing:

- `~/BridgeVM/work/canonical-fresh-12041-agent-20260730.raw`
- `~/BridgeVM/work/canonical-fresh-12041-agent-20260730-vars.fd`

Every live experiment used a `cp -c` clone. The preserved source was not booted in place. The A9 image initially lacked an ARM64 virtio-serial driver, so a separate candidate received the Microsoft WHCP ARM64 `vioser` 100.101.104.28500 package through the existing offline injector. On that candidate both viogpu3d 120.41 and vioser reported PnP `Status OK`, and bvagent connected in 26–31 seconds.

## A1 — fresh stage4 failure reproduced

On the single-generation fresh 120.41 candidate, stage4 again timed out in guest `vulkan_virtio.dll` at `vkCreateInstance`; the command returned errorlevel 13 after the 45-second timeout. This rules out accumulated DriverStore state as a necessary cause of the stage4 failure.

This was a diagnostic pilot, not a completed ten-run gate. A1 therefore remains incomplete; no reliability percentage is claimed.

## A2 — ICD loading classification corrected

The injector-created PPSSPP autostart and the manual verifier initially produced two PPSSPP processes. The Run key, stale firstboot task, and both processes were removed before the isolated test.

In the isolated `PPSSPPWindowsARM64.exe --vulkan-available-check` run:

- PPSSPP loaded the registered DriverStore copy of `vulkan_virtio.dll`.
- Host trace observed Venus context/resource/blob/submit activity.
- The process then waited indefinitely before producing a successful availability result or title frames.

Therefore the old `venus-icd-not-loaded` label was a false postcondition classification. The new boundary is after ICD load and initial Venus bring-up, inside an instance/device-availability wait. No process-attributed FPS samples were produced, so A2 remains incomplete. The existing PPSSPP log-regex FPS source is also not valid for PPSSPP 1.20.4 and must not be used as shipping evidence.

Relevant live directories include:

- `~/BridgeVM/runs/rethink-vioserial-activate-20260730-203138`
- `~/BridgeVM/runs/rethink-a2-fresh12041-baseline-20260730-201219`

## A3 — immediate E_FAIL moved to kernel bugcheck

The fresh candidate synchronized the ARM64 workload and local ARM64 DXVK `d3d11.dll`/`dxgi.dll`. Unlike the dirty 120.45 image's immediate `D3D11CreateDeviceAndSwapChain` `E_FAIL`, this run reached substantial Venus activity:

- `RESOURCE_CREATE_3D=283`
- `SUBMIT_3D=193`
- `RESOURCE_FLUSH=71`
- `fence_create=64`, `fence_complete=64`, `fence_deliver=64`
- Venus contexts 23 and 24 were created during the workload.

The guest then requested PSCI `SYSTEM_RESET`. A subsequent clone boot recovered Windows event 1001:

- bugcheck `0x000000d1` (`DRIVER_IRQL_NOT_LESS_OR_EQUAL`)
- arguments `(0xfffff803bfc23a28, 0x2, 0x8, 0xfffff803bfc23a28)`
- dump `C:\Windows\Minidump\073126-3281-01.dmp`

The recovered DXVK log identifies DXVK 3.0.2 ARM64, finds `vkGetInstanceProcAddr` in `vulkan-1.dll`, and records the enabled Win32 surface extensions before the crash. The minidump includes `viogpu3d.sys` among loaded modules, but macOS LLDB cannot parse the Windows kernel-dump format; this evidence does not guess the faulting module without symbol analysis.

Recovered dump:

- host path: `~/BridgeVM/runs/rethink-a3-dump-fetch-20260730-212413/share-host/a3-0xd1.dmp`
- SHA-256: `4846d820ef5b9b09ece1e3e28ea9292ab4616b2a5342c7b972e029d7214c7323`
- size: 386,312 bytes

Primary evidence directories:

- `~/BridgeVM/runs/rethink-a3-fresh12041-20260730-205914`
- `~/BridgeVM/runs/rethink-a3-postreset2-20260730-212255`
- `~/BridgeVM/runs/rethink-a3-dump-fetch-20260730-212413`

No FPS line, completed present sequence, or third-party-title result exists. The project-authored executable remains a transport preflight, not an A3 shipping pass.

## A8 — exact boundary is guest VidPN/mode enumeration

Live resize evidence:

- viogpu3d 120.41 reported PnP `Status OK` / `CM_PROB_NONE`.
- Host accepted `RESIZE 1600x900`.
- Config MSI-X vector 0 was raised.
- The subsequent guest `GET_DISPLAY_INFO` received `1600x900`, enabled.
- Windows desktop bounds remained `800x600`.
- Subsequent guest `SET_SCANOUT` commands remained `1024x768`; none selected 1600×900.

A separate guest Win32 mode probe reported:

```text
BV-MODE current=800x600@64
BV-MODE enum[0]=800x600@64 bpp=32
BV-MODE target_found=False
BV-MODE final=800x600
```

Because 1600×900 is absent from `EnumDisplaySettings`, a user-mode `ChangeDisplaySettings` trigger cannot request it. This isolates A8 to viogpu3d's VidPN/mode-enumeration path rather than host backing geometry, config interrupt delivery, `GET_DISPLAY_INFO`, or a missing product-shell trigger.

Evidence directories:

- `~/BridgeVM/runs/rethink-a8-fresh12041-20260730-211259`
- `~/BridgeVM/runs/rethink-a8-mode-request-20260730-211944`

A8 remains incomplete because neither guest-reported geometry nor final guest `SET_SCANOUT` matched 1600×900.

## Shipping impact

No criterion was relaxed or newly checked. A1, A2, A3, and A8 remain open. The fresh-image A/B test disproved “dirty DriverStore state is the common cause” and replaced three ambiguous boundaries with reproducible ones:

- A1: fresh guest `vkCreateInstance` timeout still occurs.
- A2: registered ICD loads and starts Venus work, then waits during availability bring-up.
- A3: fresh DXVK bring-up reaches Venus work and then Windows bugchecks 0xD1.
- A8: requested host mode reaches `GET_DISPLAY_INFO` but is absent from guest mode enumeration.
