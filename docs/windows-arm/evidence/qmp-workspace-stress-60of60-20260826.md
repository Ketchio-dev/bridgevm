# QMP full-workspace stress receipt — 60/60

This closes only the post-1.0 QMP test-socket flake acceptance item. It does
not provide guest-behaviour, graphics, pointer, glyph or release evidence.

- Exact commit: `8c7f45df90fe4c02b5392ff0fea01486dff64f7b`
- Studio job: `20260825-182319-70701-218`
- Host: Mac16,9, macOS 26.5.2
- Load: 24 concurrent load processes
- Historical-ordering baseline: 20/20 reproduced the expected
  `EINVAL`-then-`ECONNREFUSED` signature
- Acceptance workload: 60 sequential `cargo test --workspace --locked`
  rounds, 60 passed, 0 failed
- Private receipt SHA-256:
  `9ab6ca4133133bd57263a7983b13766c96059b01252fbba3f73ad27381b0f6a1`
- Public receipt SHA-256:
  `2946a2d9dee5306f389cbd092fee27d6e33bccb3b2e46ddb2bf6ce0ce8d638e3`
- Summary SHA-256:
  `43fd40a3cebc8f7c2b59963e9b255077797c65679c13d56b1fe997cb1c4a0dda`
- Baseline SHA-256:
  `05d38695f3e9805aa891e2687a89ea0efaa4f5c4912547f1b4a05cf2a21e0ddc`
- Result environment SHA-256:
  `78add040d6357543f46028b1b98baa6bae9f577264ad7950d0a5282fcd9d1ceb`

All 60 retained `round-NN.log.gz` files were independently hashed. The SHA-256
of the normalized manifest `round filename + space + compressed-byte SHA-256`
is `6f403c60cc17f99df3850c0f5b9f679b5fa47b05e989b0ef4db9fe61f53e8afa`.
Every file was also decompressed and rehashed; all 60 hashes exactly match the
per-round values in `summary.txt`. The SHA-256 of that normalized decompressed
manifest is `8ea8f135462358e9099575c4ba37e8516114486ab202574d7b652303de946d8d`.

`scripts/live-gates/verify-qmp-stress.py --out <retained-job>` passed against
the completed directory. The public and private receipts have the same JSON
object after redaction (field order differs only). The queue result is
`result=pass`, `exit_code=0`; no partial or earlier failed t10 run was combined
with these samples.
