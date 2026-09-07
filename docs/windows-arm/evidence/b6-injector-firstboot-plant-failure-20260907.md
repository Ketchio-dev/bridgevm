# B6 observation on the fixed probe: injector plant block never ran — 2026-09-07

This record retains one failed `t7-windows-closure` observation and the
injector defect it exposed. It promotes nothing. B6 stays OPEN with its
statement, thresholds and matrix unchanged; A9 is untouched.

## Sealed inputs

- tested commit: `c193b7c0eb915564eb09afa7dae9cbc060b10d89`
- job: `t7-c193b7c0-fixed-b6-observation-r1`, submitted 18:35:47Z, started
  18:36:02Z, finished 19:25:56Z
- input manifest SHA-256:
  `97271ae87ea6c953f141f70dd946496bd62997cbc6ac36d693b1f06db9db1ac4`
  (the retained `t7-bdad4cbc-b6-observation-r3` manifest with only the
  `binary` row replaced)
- sealed probe SHA-256:
  `4385f3817072ff94819a54ee002d18c5025779f67a2d17124789182232bbd974`,
  built from `13fb8213` with `recover_swallowed_vtimer_fire` removed; strict
  codesign, HVF entitlement, `virtio_gpu_3d_compiled=true` and exact
  virglrenderer linkage verified before submit
- injector SHA-256: `5ff1bba839d7511412f830124302946baf6d5b79529d89987fd4117308256dc1`
- receipt SHA-256 (private == public):
  `48c45e71c024b854fc928c647afe5609fbe2f103a3388031513eeebb58c82c3b`
- host: Mac17,9, macOS 26.5

## Result

`outcome=failed`, `pass=false`, `passes=0`, `failures=4`,
`injector_boot_observed=true`, `module_identity_verified=true`,
`f1_driver_load=false`, `f2_resize=false`, `f3_window_verbs=false`,
`f4_glyph_observation=blocked`, `active_scanout_capture=false`. The proof
lane ended on `FAIL: firstboot stage4 readiness timeout` after the full
2700-second wait: 326 `BVFIRSTBOOT_PENDING` replies, zero READY.

What the run did prove: with the fixed probe the injector clone booted and
shut down cleanly (`PSCI system off`, 178/178 successful NVMe writes, DISM
reflected `viogpu3d.inf` and `netkvm.inf` at 18:38:03Z), and the prepared
clone reached the Windows 11 desktop with the Test Mode watermark by the 60 s
ramfb checkpoint, the guest agent answering for the whole wait. The
2026-09-06 stall at `0x1bf33ba04` did not recur. The vtimer fix is not
implicated in this failure.

## Cause: `bvinject.cmd` exits its plant block on a missing `fc.exe`

All inspection was read-only (`hdiutil attach -readonly`, `wimlib-imagex dir`).

On the retained prepared disk
(`~/BridgeVM/prepared/windows-1.0/845fec6f…-bec224d2…`), `C:\BridgeVM`
holds the copied package and `bvgpu-clean-driver-state.ps1` in both
`C:\BridgeVM` and `C:\BridgeVM\viogpu3d\`; the stage1–3 flags, boot receipts
and `viogpu3d-firstboot.log` that the source image carried are gone. That is
every line of the plant block up to the copy. Nothing after it ran:
`viogpu3d-firstboot-pending.flag` is absent, and a UTF-16 scan of the
offline `SYSTEM` and `SOFTWARE` hives finds neither
`BridgeVMGpuDiagnosticsProbe6` nor `BridgeVMGpu3DStage1`
(the source image scores zero on both too, because the firstboot runner
consumes and deletes them).

The sealed injector's `boot.wim` image 2 — the one `winpeshl.ini` launches —
contains a `bvinject.cmd` byte-identical to the repository's
(`3849bc93…`), and its `System32` has no `fc.exe`, `comp.exe`,
`findstr.exe`, `certutil.exe` or `powershell.exe`. The line after the copy
was `fc /b … >nul`; cmd reports 9009 for an unknown command,
`if errorlevel 1` fires, `BVINJECT ERROR: package-local cleanup verification
failed` goes to the (serial-less) WinPE console, and `goto :end` shuts down
before the pending flag, the activation service and the RunOnce entry exist.

The guard entered in `db81c72c` (2026-08-29 06:51 EDT). The sealed injector
was built the same day at 21:30 EDT and carries it; the injector behind the
2026-08-20 F1–F4 PASS (`5eaedee1…`) predates it. Every `t7` job since has
therefore re-injected a guest on which firstboot could not run. The
2026-09-06 `t7` observations failed earlier, in firmware, under the vtimer
defect, so this was first visible today.

## Change

`scripts/win-assets/bvinject.cmd` now checks the copy's exit status, compares
file sizes with `%~z`, and runs the same `find /c "[CmdletBinding()]"` header
check that `bvgpu-firstboot.cmd` already applies (`find.exe` is in WinPE).
`tests/integration/hvf-windows-viogpu3d-firstboot-asset-smoke.sh` pins the
new guard, and the new
`tests/integration/hvf-windows-injector-winpe-commands-smoke.sh` tokenises
every command the injector invokes and fails on anything outside the cmd
builtins plus the executables measured present in the sealed WinPE image
(`reg`, `dism`, `xcopy`, `find`, `wpeinit`, `wpeutil`). Mutation-tested:
the pre-fix injector fails it on `fc`; appending `if exist x certutil …`
and a `for … do … powershell …` line fails it on both.

## What this does not establish

No F1–F4 observation and no glyph scene was captured. A new injector must be
built from the fixed head, a new manifest sealed with its hash, and the tier
re-run before B6 can be observed at all. The secondary observation that every
proof-lane `SET_SCANOUT` (1280x1024 against an 800x600 device) answered
`ERR_UNSPEC` is the known pre-`RESIZE 1600x900` state, not a new defect.
