# T19 publication and installer dry-run contracts — 2026-10-03

Evidence rank: deterministic tests and independent code review. No Windows
guest, physical gate or criterion campaign was run for these changes.
The baseline is `955c248df28e5d201b930e14338a506f640b3aed`.
Product truth remains in
[`capabilities/windows-hvf.json`](../../../capabilities/windows-hvf.json).

## Failed experiments retained

The existing installer policy assertion ended with `|| true`. Exact copied
fixtures returning exit 42, omitting the success banner, or writing an owned
HOME file were all accepted. Original probe log SHA-256:
`06e011935cba95cb6f41665962cb22ec0bfba38f7dcdce643f1247126007685f`.
The new regression suite before the policy correction passed 10 tests and
failed its exit-42 assertion; log SHA-256:
`158a997ecc514b05151c2ebf805271115f06ef8d3d4bced60ac8f4bac06f67f0`.
This establishes an inadequate check, not a production installer failure.

Valid synthetic completed and blocked T19 receipts passed private validation
but failed actual public publication because the redactor dropped ten required
fields. The signing-class validator also accepted values outside the existing
schema enum. The initial nine-method regression run retained 28 failed
assertions; log SHA-256:
`dc0fec6af540e6bf1fccc283e70f4e223b9fa998261e18e04d9e0b6061cf6196`.
An earlier fixture collection failed on a Python reserved-keyword typo before
running a production assertion; it is a separate harness failure, retained at
SHA-256 `82aa2683e9db5f79fc6421417cf97f9a5c88fb290e4c93159699477c20905103`.

## Corrections and focused proof

Installer conclusion `26fb146c` runs exact-byte copied Bash installers with
owned HOME, queue and source fixtures and a restricted tool directory. It
checks the exit status, exact success banner, unchanged file contents/modes,
and absence of installation/service commands. Runner, privacy-path and missing
prerequisite refusals remain tested. The actual policy function and assertion
must reject an exit-42 contract command. Production installer bytes are unchanged.
All 11 tests passed; the complete policy reported 101 passing checks, log
SHA-256 `54675733939246e9650ffdfa12dfde3cb09f5554fd9fac05b689fc419583cc6a`.

T19 conclusion `4eac0eae` adds exactly seven established hash fields and three
stage counters to the public allowlist, preserving all 363 previous keys:

- `source_disk_sha256`, `source_vars_sha256`, `source_vtpm_tree_sha256`;
- `imported_initial_disk_sha256`, `imported_initial_vars_sha256`,
  `imported_initial_vtpm_tree_sha256`, `final_vtpm_tree_sha256`;
- `source_authenticated_passes`, `ui_imported_passes`,
  `imported_media_authenticated_passes`.

The validator accepts only the existing four signing-class strings and refuses
unknown strings and other JSON types. Actual publisher contracts cover pilot
and release-shaped fixtures, blocked/missing receipts, private metadata,
commit mismatch, no-overwrite destinations and temporary-file cleanup.
All 11 publication/security/provenance tests and existing receipt checks passed;
focused log SHA-256:
`d274b3f70129f3900ea2f693c40ca0c0cb03cef787de7010a5f96afc06465473`.
The extracted allowlist is included in T16 source seals and the T17 copied
fixture. Existing redactor, import, cleanup and NVMe reporter checks passed.

Independent reviews found no required changes in either conclusion. New files
are registered at actual sizes; no existing structural ceiling was raised.
All source, budget and documentation inputs precede the capability identity
seal. Full local project and exact-head hosted results must be recorded in the
change PR before treating this tranche as verified.

These synthetic receipts do not establish installation, imported guest boot,
shutdown, clean-machine behavior or signing-policy eligibility. All criterion
states, thresholds, promotion flags and product wording remain unchanged.
