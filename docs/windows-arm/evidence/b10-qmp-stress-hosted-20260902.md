# B10 hosted QMP full-workspace stress (2026-09-02)

## Result

B10 passed its unchanged deterministic criterion at exact commit
`350a7e55457e8a60c069ce00287c74d9e9e23da5`.

- GitHub-hosted run: `33583360863`, attempt 1
- Artifact: `qmp-stress-33583360863-1`
- Host: ARM64 `macos-15`, macOS `15.7.7`
- Rust: `1.97.0`
- Load processes: 24
- Negative control: 20/20 reproduced `EINVAL` then `ECONNREFUSED`
- Full workspace command: `cargo test --workspace --locked`
- Required/passed rounds: 60/60, zero failures
- Receipt SHA-256:
  `c62e72fc83393bdb63b45e6f6c86a310f72f497440d3b4f5ceb7f7939a6a1660`
- Baseline SHA-256:
  `05d38695f3e9805aa891e2687a89ea0efaa4f5c4912547f1b4a05cf2a21e0ddc`
- Rounds-manifest SHA-256:
  `710c8689e21d93c0021a0a15e562f386fbcd53eeb07ca1089aa7196f27f27d37`
- Summary SHA-256:
  `a7609cc1b028ac1278bc1f500da497f28793b8c558b3aa83a0ea93925da09dd1`

The downloaded artifact passed the strict verifier with the expected
repository, workflow, run, attempt, workflow-head SHA and commit. The verifier
rehashes every compressed log and its decompressed raw bytes. A separate shell
audit counted exactly 60 manifest rows and 60 compressed logs, then independently
matched all 60 gzip hashes and all 60 raw hashes.

The contract self-test also passed and retains negative mutations for 19
controls, 59 workspace rounds and a changed retained log. The relevant workflow,
runner and verifier files have no diff between the tested commit and the
2026-09-02 evidence review head.

## Retention and scope

The GitHub artifact retains the 60 compressed logs for 30 days. The repository
retains the immutable receipt and its root hashes. This is deterministic
host-process and workspace evidence, not a live guest run. It closes only B10;
it does not prove Windows boot, graphics, product E2E or performance improvement,
and it does not promote the Engineering Preview product state.
