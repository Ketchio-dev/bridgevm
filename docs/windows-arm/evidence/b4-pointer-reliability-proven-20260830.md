# B4: fixed 20-lane pointer gate passes 20/20 (2026-08-30)

## Result

Studio tier `t8-pointer-reliability` job
`20260830-134316-74417-14894` passed the unchanged B4 acceptance gate on a
real `Mac17,9` host running macOS 26.5 at exact BridgeVM code head
`e38ba7c9dc252d13977a4e3c69564cc88abb1b35`:

- sample count: 20 independent disk and UEFI-vars clones;
- pointer samples: 20;
- guest-observed landed clicks: 20/20;
- rendering/package regressions: 0;
- stuck buttons: 0 in every lane;
- first-visible-change p95: 243 ms, against the fixed 250 ms limit.

Every lane recorded one host-fired absolute click, at least one guest
`BVPTR press` and `BVPTR release`, exactly one target click, `stuck=0`, and a
change from the active CGL IOSurface baseline. Lane 8 was a retained 272 ms
outlier. For N=20 the gate's predeclared nearest-rank p95 is the nineteenth
ordered sample, 243 ms; the outlier was not rewritten or excluded.

The terminal gate summary was:

```text
landed 20/20 rendering_package_regressions=0 required_pointer_samples=20 p95_first_changed_ms=243 (limit 250)
B4 pointer reliability: PASS
```

The private receipt was successfully reduced to the checked-in
[public receipt](b4-pointer-reliability-receipt-20260830.json), with
`outcome=completed` and `pass=true`. Its sealed identities are:

| input | SHA-256 |
| --- | --- |
| prepared pointer image | `5cec12c7fd7f5ec32e3a284c59ce28d827f0a932709854831d1da119021d90df` |
| prepared pointer vars | `bec224d27c8681d2db69583e933e2d99b6fa5265d91d37373cb7a2c8b71853cd` |
| driver package tree | `838fcbe165dbdae0f18fa1cf21b156b0114f49f1041b2df367541413b00aa9f1` |
| submitted input manifest | `75caf98b80b95b8e5a7a31f6ed845dcbeaaa409ab7aa2013d1c4f32b84323c5a` |
| public receipt | `59d79bce7e20fbd1fdd1facf9898557b9c4562301e168864ac59a8e2b8c4ae80` |
| complete gate log | `206a6918a06f10f07c16ed74623ec37a8cfa075446656f6cb3a7ff4e158f6852` |

The receipt finished at `2026-08-30T18:07:24Z`. No lane `run.log` or
`virtio-gpu.jsonl` contained `Illegal resource` or `context error reported`.

## Candidate and fresh-install provenance

The experimental test-signed package came from builder repository
`Ketchio-dev/viogpu3d-arm64-builder`, branch
`fix/b4-ctx-attach-queue-retry`, commit
`7407eafeddc029c93e56081fedb25388f23c52ed`. Hosted build
`33325014273` passed, and the package checker passed the downloaded full
artifact. Important component hashes were:

| component | SHA-256 |
| --- | --- |
| KMD `viogpu3d.sys` | `97ceb764ef2501da2b4462ad9531b8b6798af1d590c3ad0a9c714999ed2c2a64` |
| D3D UMD `viogpu_d3d10.dll` | `90096e2f073308af4b220358c5266eacbdbeb5af4467b27f1cf7c91b09fdd6b7` |
| Vulkan ICD `vulkan_virtio.dll` | `6bc4caec202324475ce4201705d0f9e3b2432ddee39f4e7c97617246e6b7395e` |
| INF `viogpu3d.inf` | `efbf195e5a97a5ebc93b2e5739e73d688b294e39c27bb8e6714761c58d2bf6ba` |

Before T8, the package was installed into a fresh canonical-image clone using
four separate VM processes. Stages 1-3 each recorded agent start, the expected
guest reboot log, launcher exit 42, and persisted disk/vars state. Stage 4
recorded `stage4_pass=1`, `BVGPU-DRIVERSTORE-CLEAN-PASS`,
`BVGPU-DRIVER-STATE-PASS`, bound version `120.47.0.0`, and identical expected,
bound, and host INF hashes. No stage `run.log` or `virtio-gpu.jsonl` contained
either renderer-error marker.

The installed stagewise source was sealed as image
`5ad7a304cfec4fe9320784b26b4d6895885361ddef2675d2411a759cb54165f8`
and vars
`2f0e68923bf0e4cc1bcfd51a6bb67eb661d48b90aef97ea3b03d4b2805b33ca7`.
Its exact driver/image/vars manifest has SHA-256
`75caf98b80b95b8e5a7a31f6ed845dcbeaaa409ab7aa2013d1c4f32b84323c5a`.
T8 then performed its own clean preparation boot and sealed the distinct
pointer-source hashes recorded in the public receipt above.

The candidate makes control-queue context attachment report `NTSTATUS`, retry
the same queued buffer for bounded queue-exhaustion intervals, propagate
failure closed through allocation open, and detach only resources that were
successfully attached. This extends the earlier backing fix to the context
attachment that precedes general submits.

## Claim boundary and retained failures

This live batch proves that the exact BridgeVM code, package, prepared source,
and per-lane clones named by the receipt satisfy B4. It does not prove that
transient control-ring exhaustion was the exclusive cause of every historical
black lane.

The prior complete result, job `20260825-032534-61218-23891` at
`0ea368e90f945d5c40d414a2b9ea886faad94878`, remains a failed historical
experiment: 9/20 with p95 704 ms. Its all-black and invalid-target lanes remain
rendering/package regressions and are not reclassified as pointer failures or
successes. Later 120.45 campaigns that missed the fixed sample or latency gate
also remain failed experiments in the
[follow-up record](b4-pointer-followup-session-variance-20260830.md).

This package is self-signed and test-mode-only. It is B4 experimental evidence,
not Microsoft kernel-policy signing provenance, and does not close A9.
