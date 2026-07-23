# Windows ARM64 packaged-app PPSSPP Venus live receipt (2026-07-23)

## Result

`GPU-LIVE-RECEIPT` is live-proven on Mac Studio [A] for the project's PPSSPP
real-application boundary. A packaged BridgeVMControl app booted a clean
120.41 Venus lineage, bound the exact test-signed ARM64 driver, launched native
ARM64 PPSSPP 1.20.4 on the Vulkan/Venus path, kept its rendered UI alive for
more than ten minutes, measured framebuffer delivery, and shut Windows down
cleanly with NVMe writeback.

Evidence directory:
`~/BridgeVM/runs/wall-c8-clean-ppsspp-600s-20260723/`

## Packaged-app and driver inputs

```text
packaged executable sha256=c95527e62e64f2903cdd25b88884e331b546fd562221f90e45b34c0af6101f31
performance_risk=aggressive
virtio_gpu_3d=1
virtio_gpu_pci_device_id=10F7
state_encryption=aes-256-cbc-etm/key-fd
DriverVersion=120.41.0.0
InfName=oem33.inf
expected_inf_sha256=2CD1735D0E0B79F42CC75FE8479773D198C06142862875C2C6FAD3D9C7A45C40
bound_inf_sha256=2CD1735D0E0B79F42CC75FE8479773D198C06142862875C2C6FAD3D9C7A45C40
```

This run used a clone of the pre-120.33 image and injected only the known-good
120.41 package plus PPSSPP. That avoided the accumulated DriverStore state that
had made later 120.43 and mixed-upgrade lineages crash during PPSSPP startup.

## PPSSPP survival and rendering

The retained framebuffer shows the PPSSPP 1.20.4 native ARM64 UI, with no
"PPSSPP crashed while starting" or Vulkan-to-D3D11 fallback dialog. The final
normal-running frame was captured after more than ten minutes. The host boot
timer continued recording changing checksums and roughly 34,000–35,000 unique
colors through elapsed 814 seconds; shutdown began at 815 seconds and the run
ended at 827.638 seconds.

Host trace summary:

```text
protocol=venus
parsed_events=115640
invalid_lines=0
3D backend attached=true
VENUS feature set accepted=true
GET_CAPSET VENUS id 4=true
SUBMIT_3D non-empty=true
fence create/complete/deliver=true
scanout readbacks=11293
scanout IOSurface blits=11293
P3 Windows 3D trace gate=PASS
Blockers=none
```

The trace report lists recoverable `RESOURCE_UNMAP_BLOB` error responses, but
the P3 gate passed and the application/render path continued for the full run.
They are not hidden as a zero-error protocol claim.

## Frame-rate metric

`scripts/fb-rate.py` measured a continuous 300-second window during the same
packaged-app boot:

```text
RESULT frames=4061 seconds=300.0 avg_fps=13.54 min_1s=4.0 max_1s=23.0
```

The requested acceptance criterion is title survival plus a measured delivery
rate, not a 60 FPS performance target. This receipt therefore proves the live
path and records its current performance without overstating it.

## Shutdown and persistence

```text
stop=PSCI 0x84000008 (system off)
NVMe disk written back
storage writes=188/188 successful
flushes=16
pending_without_completion=0
TPM PCR_Read=438 PCR_Extend=243
TPM backend_failures=0
malformed_commands=0 malformed_responses=0
```

## Negative follow-up boundary

A later attempt to make the generic manifest evaluator itself wait 600 seconds
was intentionally fail-closed and did not replace this successful receipt.
The first attempt hit the probe's old hardcoded 50,000,000 per-vCPU exit cap;
a
new explicit `--max-exits` policy now preserves the 50M default and permits a
bounded 150M long-run override. The next fresh-driver attempt then stalled in
stage-4 Vulkan probing before the PPSSPP title gate and was recorded as failed
(`wall-c8-final-real-title-600s-20260723` and
`wall-c8-proof-real-title-600s-20260723`). Neither failed run is counted as
positive evidence. The positive C8 claim rests on the packaged 120.41 run above,
which visibly ran PPSSPP for over ten minutes and produced the same-boot rate
metric and clean shutdown receipt.
