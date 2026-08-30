# B4: fixed 20-lane pointer gate passes 20/20 (2026-08-30)

## Result

Studio tier `t8-pointer-reliability` job
`20260829-215356-7576-17586` passed the unchanged B4 acceptance gate on a
real `Mac17,9` host at sealed BridgeVM commit
`080462846acbe4cb784bd9b532d7cd39921aa549`:

- sample count: 20 independent disk and UEFI-vars clones;
- pointer samples: 20;
- guest-observed landed clicks: 20/20;
- rendering/package regressions: 0;
- stuck buttons: 0 in every lane;
- first-visible-change p95: 245 ms, against the fixed 250 ms limit.

Every lane recorded one host-fired absolute click, at least one guest
`BVPTR press` and `BVPTR release`, exactly one target click, `stuck=0`, and a
change from the active CGL IOSurface baseline. Lane 18 was a 745 ms outlier.
For N=20 the gate's predeclared nearest-rank p95 is the nineteenth ordered
sample, 245 ms; the outlier was retained and was not rewritten or excluded.

The retained summary was:

```text
landed 20/20 rendering_package_regressions=0 required_pointer_samples=20 p95_first_changed_ms=245 (limit 250)
B4 pointer reliability: PASS
```

The private receipt was successfully reduced to a public receipt with
`outcome=completed` and `pass=true`. Its sealed identities are:

| input | SHA-256 |
| --- | --- |
| prepared pointer image | `7385d2005f3c48c40519a2d9cf2b9f975a3763ad8f37e84937d5ddf8429b0f8b` |
| prepared pointer vars | `bec224d27c8681d2db69583e933e2d99b6fa5265d91d37373cb7a2c8b71853cd` |
| driver package tree | `c46486e5643317002b8848ac855324bc397e6ee6ac6e2987da7af0c61c6a3860` |
| submitted input manifest | `9d5844483ef72d9c240e5ccff473089b4b51b2533e232b95f322503fa4b19f94` |

## Candidate and fresh-install provenance

The experimental test-signed package came from builder repository
`Ketchio-dev/viogpu3d-arm64-builder`, commit
`56df7089eb235ce9d6f9e96746d4fe9d120a2739`, hosted build
`33285376389` (PASS), full artifact `9724362749`, GitHub artifact digest
`e8662716f578177d79348c7cdccfddac8c40ff0167ca6e383e710a5442b04997`.
Important component hashes were:

| component | SHA-256 |
| --- | --- |
| KMD `viogpu3d.sys` | `45c56966766e0c40bd1910d8d37fdf4b4f282f4ab83a8f966e5e6ae371ab380a` |
| D3D UMD `viogpu_d3d10.dll` | `1310a116d11ea099ccd4b19a9f9637ebbaea2a04cff040ae2f77764b6ed1b62e` |
| Vulkan ICD `vulkan_virtio.dll` | `f4a176aaa7df8673eb4b87d085e4a4310c342cd9ca568f2ecc14e955c3c5d787` |
| INF `viogpu3d.inf` | `1e7502dc11268911f8fc1bbb1981fa5992c07143b494dd13bab796fcaac76e73` |

Before T8, the package was installed into a fresh clone using four separate VM
processes. Stages 1-3 each recorded agent start, the expected guest reboot log,
launcher exit 42, and persisted disk/vars state. Stage 4 recorded
`stage4_pass=1`, `BVGPU-DRIVERSTORE-CLEAN-PASS`,
`BVGPU-DRIVER-STATE-PASS`, bound version `120.45.0.0`, and identical expected,
bound, and host INF hashes. No stage `run.log` or `virtio-gpu.jsonl` contained
`Illegal resource` or `context error reported`.

The installed stagewise source was sealed as image
`0bcd42dd53803ce727af5d6fab8eef6c96a585c5789118b014f3c7c650cbc7c1`
and vars
`2f0e68923bf0e4cc1bcfd51a6bb67eb661d48b90aef97ea3b03d4b2805b33ca7`.
T8 then performed its own clean preparation boot and sealed the distinct
pointer-source hashes recorded in the public receipt above.

## Claim boundary

The candidate changes make `RESOURCE_ATTACH_BACKING` queue exhaustion
fail-closed and boundedly retry the control-queue submission. The passing live
batch proves that this exact package and prepared source satisfy B4. It does
not, by itself, prove that transient control-ring exhaustion was the exclusive
cause of every historical black lane.

The prior complete result, job `20260825-032534-61218-23891` at
`0ea368e90f945d5c40d414a2b9ea886faad94878`, remains a failed historical
experiment: 9/20 with p95 704 ms. Its all-black and invalid-target lanes are
not reclassified as pointer failures or successes.

This package is self-signed and test-mode-only. It is B4 experimental evidence,
not Microsoft kernel-policy signing provenance, and does not close A9.
