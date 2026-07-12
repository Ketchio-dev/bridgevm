# In-process Windows reboot and GIC reset live proof — 2026-07-12

This record closes the interrupt-controller part of the BridgeVM HVF
`PSCI SYSTEM_RESET` path. It indexes a preserved mutable-media run; it is not a
claim of durable suspend or production driver readiness.

## Root cause

The existing reboot loop stopped and joined every secondary vCPU, reset the
BridgeVM platform devices, cleared guest RAM, and restored CPU0's power-on
registers. It did not reset Apple's in-kernel GICv3 device. A long Windows WDK
installation triggered a guest reboot and the second firmware generation then
remained indefinitely at the TianoCore boot-option screen. Identical ramfb
checksums at 1, 30, 60, and 120 seconds confirmed that this was not merely a
slow boot.

Apple documents `hv_gic_reset()` as the VM-reset operation for the GIC
distributor, redistributors, and internal device state. The reboot path now
calls it only after all secondary vCPUs have stopped and joined and CPU0 is
outside `hv_vcpu_run`. A nonzero return stops the run with an explicit failure
instead of continuing with partially reset interrupt state.

## Observed result

- The first Windows desktop agent emitted `READY` at 20,480 ms.
- `shutdown.exe /r /t 0 /f` exited 0 and produced `PSCI SYSTEM_RESET: reboot
  1/8`.
- `hv_gic_reset` returned `0x0`.
- Firmware and Windows restarted in the same VMM process; the second agent
  emitted `READY` at 17,785 ms.
- A second `ver` command completed with exit 0, followed by an exit-0
  `shutdown.exe /p /f` and PSCI system-off.
- The VMM wrote back both UEFI vars and the 48 GiB NVMe target.
- `agent-service-gate.txt` recorded agent handshake, command completion,
  service start, guest system-off, NVMe writeback, `probe_status=0`, and
  `status=0`.

The reboot decision model has a separate `reset_gic` action, and its unit test
requires GIC, platform, guest RAM, and vCPU reset before another boot
generation is allowed.

## Preserved evidence index

The local evidence directory at capture time was:

```text
/Users/user/BridgeVM/hvf-gic-reset-proof-20260712-v1
```

| File | SHA-256 |
| --- | --- |
| `run.log` | `8698207faaebe15a1647629bad5d09aa43c9fa658e1a7f3f72bba174638751c9` |
| `agent-service-gate.txt` | `b76d129a6b1b10d95d1db86d7cb52b9ebdd7ad1bc5672d6b47b870daeb9aeade` |
| `preflight.txt` | `de3aa17c5f52f096f4234f42c32353bc52b38223f0f6b15f4e86c345778f0df6` |
| `target-stat.txt` | `16001d9cdff0780f660a74bc9995bf872e52ce78b984b0a1fd33207f8b950cae` |

## Limits

This proves one installed-Windows warm reboot, GIC reset, second-generation
agent recovery, clean power-off, and disk/vars writeback in one process. It
does not serialize GIC or vCPU state for disk-backed suspend, prove arbitrary
numbers of consecutive resets, or prove that the interrupted WDK installation
itself completed.
