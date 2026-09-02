# B7 CoreAudio teardown live campaign (2026-09-01)

## Result

B7 passed its unchanged fixed ten-run criterion at exact green seal
`350a7e55457e8a60c069ce00287c74d9e9e23da5`.

- Studio job: `20260902-023340-t18-350a7e55`
- Required/passed: 10/10, with no replacement lanes
- Total rendered frames: 2,504,031
- Drops: 0
- Unexpected callback errors: 0
- Active, queue-invalidated and unclassified callback errors: 0
- AudioQueue stop errors: 0
- AudioQueue dispose errors: 0
- Expected stopping statuses: 30, all typed `EnqueueDuringReset`
- Worker cleanup: verified
- Input manifest SHA-256:
  `3a8cf8ed9fa5d134fac6ef6b345816df01e70a24276fdbe91e826dd79244091a`
- Sealed binary SHA-256:
  `5912a1f291d9935d3d4d2270b37829de543645db2583c3e3f257b9296f1493e8`
- Run-log-set SHA-256:
  `dd4780eb4a329a8bee97fbab2e307451733584becefc92a12fc8c4fb5176d853`
- Public receipt SHA-256:
  `5844b7a086e7a3877ec34c0365331f23ea0f2baa2f62b0eab79d1ea0de491081`

The public receipt independently passed
`scripts/verify-audio-teardown-receipt.py` with the exact expected commit. A
private cross-check found ordinals 1 through 10 exactly once, ten distinct
nonces, ten passing lane results and no surviving cloned media after cleanup.

## Scope boundary

The receipt intentionally fixes `claim_eligible=false` and
`capability_promotion=false`. It proves the non-blocking B7 teardown criterion;
it does not promote product state or prove A9, the complete application E2E,
signed Windows 3D driver acceptance, glyph correctness, the host matrix, the
20-workload matrix or QMP stress. Product state remains Engineering Preview.
