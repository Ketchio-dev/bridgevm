# Windows Venus `ResFlush` 0xD1 resolution (2026-07-19)

## Status

Resolved. The Windows ARM64 guest no longer bugchecks when the display flip path
calls `CtrlQueue::ResFlush` at `DISPATCH_LEVEL`.

This closes the specific `DRIVER_IRQL_NOT_LESS_OR_EQUAL` (`0xD1`) kernel crash.
It does not claim that the complete Venus/Vulkan presentation path is finished.

## Root cause

The 120.31 driver crashed at guest PC `0xfffff80150a63a28`. With the loaded
driver base `0xfffff80150a50000`, the failing RVA was `0x13a28`. The matching
PDB resolves that address to `CtrlQueue::ResFlush`.

`VioGpuVidPN::Flip` calls `ResFlush` while holding the source spin lock, so the
call runs at `DISPATCH_LEVEL`. `ResFlush` was nevertheless emitted into the
pageable `PAGE` section and contained `PAGED_CODE()`. Entering that code at the
raised IRQL caused the reproducible `0xD1` bugcheck after roughly 25 seconds.

## Fix

Builder commit `081ff0a336fd5910cc65d2fc58c735a657bc7234` keeps only
`CtrlQueue::ResFlush` resident outside the pageable section and removes its
`PAGED_CODE()` assertion. The CI build succeeded as run
`29682756307` and produced driver version `120.32.0.0`.

The built KMD SHA-256 is:

```text
42b80856e2a447a40ad625fd08204af03044f9d1cc2182e20fb0eea32df1d83b
```

## Verification

120.32 was injected offline into:

```text
/private/tmp/bridgevm-r10-20260718/full-live/venus-full.raw
```

The installed `Windows/INF/oem32.inf` identifies version `120.32.0.0`. Its
SHA-256 matches the injected INF exactly:

```text
adfcdfdb066a1e5830adc016f6dc5c14decec6a6b056fa29b998e9cdaab50400
```

The decisive clean-boot evidence is stored at:

```text
/private/tmp/bridgevm-r10-20260718/full-live/evidence-verify-120.32-resflush-nonpaged
```

That run used a 70-second watchdog and allowed at most one reboot. It completed
with:

- zero guest resets
- zero bugcheck or `0xD1` reports
- cleanup status 0
- a valid 60-second checkpoint
- 302 `RESOURCE_CREATE_3D` commands
- 313 `CTX_ATTACH_RESOURCE` commands
- 128 `SET_SCANOUT` commands
- 130 `RESOURCE_FLUSH` commands
- 2 `SUBMIT_3D` commands

The fixed `ResFlush` path therefore ran at least 130 times, and the guest stayed
alive for more than 2.5 times the former crash interval. The activation boot's
Windows System event log also contains no new bugcheck after the 120.32 install.

## Remaining 3D work

The kernel crash is closed, but the broader P3/Venus gate is still open. The
current trace has no fence completion/delivery, contains invalid
`RESOURCE_UNMAP_BLOB` responses, reports no scanout readback, and does not prove
a visible non-black desktop or a working guest Vulkan ICD. The next work should
focus on fence/display presentation and Vulkan end-to-end validation, not on
additional changes to `ResFlush`.
