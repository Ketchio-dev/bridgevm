# A3: viogpu3d 0xD1 AttachBacking pageable-code crash fixed

Status: crash removed and verified live. A3 itself is still incomplete — the
shipping criterion additionally requires a third-party title with
process-attributed guest FPS (`samples>0`, `p50>=30`), which this run does not
provide.

## Defect

`DRIVER_IRQL_NOT_LESS_OR_EQUAL` (`0x000000d1`) reset the guest during the D3D11
path. WinDbg (`kd.exe`) on the ARM64 triage dump resolved the faulting address
inside the shipped 120.41 `viogpu3d.sys`:

```
FAILED_INSTRUCTION_ADDRESS: viogpu3d+0x13a28
STACK_TEXT: viogpu3d+0x13a28 <- viogpu3d+0x5114
```

Section layout of that binary (`llvm-objdump -h`, ImageBase `0x140000000`):

| section | RVA       | size      |
| ------- | --------- | --------- |
| `.text` | `0x1000`  |           |
| `PAGE`  | `0x13000` | `0x13b41` |

`+0x13a28` therefore lives in `PAGE`, and the call into it comes from nonpaged
`.text`:

```
140005110: 94003a46    bl  0x140013a28
```

The `PAGE` function builds a virtio-gpu command with `hdr.type = 0x106`
(`VIRTIO_GPU_CMD_RESOURCE_ATTACH_BACKING`), identifying it as
`CtrlQueue::AttachBacking` (`viogpu/common/viogpu_queue.cpp:379`, guarded by
`PAGED_CODE()`), reached from `VioGpuAdapter::GpuObjectAttach`
(`viogpu/viogpudo/viogpudo.cpp:3815`). Executing it at IRQL 2 with the page
non-resident faults.

The pre-existing `viogpu3d-venus-dispatch-nonpaged.patch` had already de-paged
`DescribeAllocation`, `FlushToScreen`, and `ResFlush`, but not `AttachBacking`.

## Fix

`AttachBacking`'s definition was moved to a new translation unit,
`viogpu/common/viogpu_queue_nonpaged.cpp`, which contains no `PAGED_CODE` and
never opens the PAGE segment, so the linker cannot place it there. Pragma
repositioning, `__declspec(code_seg(".text"))` on the definition and on the
header declaration, and a custom section merged via `/MERGE` were each tried
first and each still landed in `PAGE`; a separate TU is what actually holds.

Two build-system defects were fixed alongside, both of which had been silently
producing unmodified binaries:

- The patch regeneration script stripped the leading space from blank context
  lines, so `git apply` reported `corrupt patch ... :8`.
- The workflow did not check `$LASTEXITCODE` after the native `git apply`, so
  PowerShell continued past that failure. Five builds were wasted before this
  surfaced.

A fail-closed workflow step now parses `llvm-pdbutil dump -publics` and throws
unless the symbol resides in segment `0001`.

## Build

Run `30602290863`, commit `221de96`, branch `fix/attach-backing-nonpaged`
(off the verified `fix/backing-only-on-12041` 120.41 baseline).

```
?AttachBacking@CtrlQueue@@...   0001:8856     (.text)
scripts/check-hvf-windows-viogpu3d-package.sh -> PASS: viogpu3d package is injection-ready
```

`viogpu3d.sys` sha256 `4ce6fbb2afb1ded968eb06135ca41017fc0813686adf0f8c94caeb029b904127`,
DriverVer 120.45.0.0, protocol venus, PCI 10f7.

## Guest installation

The WinPE injector path could not be used: its UEFI boot entry had been
outranked by the installed Windows entry, and three attempts booted
`HD(1,GPT,7985BAE0-...)` instead of the injector's own
`HD(1,GPT,A0A0C780-...)`, reported as
`injector_boot_observed=false-booted-7985bae0cbc540cfadff257ae6e6ed1f`.

The package was instead installed in-guest with `pnputil` over the existing
agent share, in run `attach-install3-20260731-074422`. `viogpu_d3d10.dll` is
larger than the share's per-file limit and was sent in three parts and
reassembled in the guest.

```
BV-ASM d3d10 size=11882920 sha256=FC11472703E48ADACC37008C53A9D54947AAEF726564A409317B02983C672560
BV-ASM sys sha256=4CE6FBB2AFB1DED968EB06135CA41017FC0813686ADF0F8C94CAEB029B904127
BV-PNPUTIL Driver package added successfully.
BV-PNPUTIL Published Name:         oem4.inf
BV-PNPUTIL Driver package installed on device: PCI\VEN_1AF4&DEV_10F7&SUBSYS_11001AF4&REV_01\3&11583659&0&28
BV-PNPUTIL exit=0
BV-STORE C:\Windows\System32\DriverStore\FileRepository\viogpu3d.inf_arm64_6435ce2e01767d8f\viogpu3d.sys sha256=4CE6FBB2AFB1DED968EB06135CA41017FC0813686ADF0F8C94CAEB029B904127
BV-GPU name=Hardsoft VirtIO GPU 3D controller (venus) drv=120.45.0.0 status=OK
```

The DriverStore copy matches the built binary byte for byte. The resulting
powered-off image pair is preserved as
`~/BridgeVM/work/canonical-attach-resident-20260731.raw` and `-vars.fd`.

## Result

Same D3D11 workload, old driver versus fixed driver:

| run                                        | driver | `RESOURCE_ATTACH_BACKING` | PSCI resets |
| ------------------------------------------ | ------ | ------------------------- | ----------- |
| `attach-backing-resident-a3-preflight-20260731-002639` | 120.41 shipped | 273 | 1 |
| `attach-d3d11-preflight-20260731-075313`   | AttachBacking-resident | 307 | 0 |

The old run's single reset is the `0xD1` bugcheck, confirmed by the guest's own
event log:

```
The computer has rebooted from a bugcheck.  The bugcheck was: 0x000000d1
(0xfffff802be5a3a28, 0x0000000000000002, 0x0000000000000008, 0xfffff802be5a3a28).
```

The fixed driver executes the very command that used to fault, 307 times, with
zero resets. The crash is gone.

Note on the earlier misleading dump: `attach-backing-dump-fetch2-20260731-033917`
produced a `0xD1` that appeared to survive the fix. It did not — that guest was
still running the old driver, because the injector had never booted. The dump's
`+0x5114` return address falls inside a function epilogue in the new binary and
cannot be a call site there, while in the old binary `140005110: bl 0x140013a28`
matches exactly.

## Not yet done

The D3D11 verifier did not reach its FPS markers. The workload was dispatched
and never returned: exactly one `BVAGENT CMD` was accepted, no matching
`BVAGENT END` followed, and the agent reported `overdue ctl awaiting-reply=true`
28 times before the watchdog stopped the run. The guest stayed healthy
throughout — zero resets, 2023 virtio-gpu commands, 64/64/64 fences — so this is
a hang inside the guest-side D3D11 path, not a transport or logging artifact.

Whether that hang predates this fix is not yet established: the old driver
always bugchecked before reaching this point, so there is no comparable
old-driver observation.

A3 remains incomplete until a third-party title reports `samples>0` and guest
`p50>=30` on the intended DXVK/adapter path.
