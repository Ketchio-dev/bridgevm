# Windows HVF clean-machine matrix contract

Status: acceptance contract only. Existing retained evidence is from one
Mac16,9 (M4 Max) host, so M1–M3 cells remain UNTESTED and this contract does
not claim the matrix passes.

BridgeVM supports Apple Silicon M1 or later. The clean-machine gate therefore
requires one distinct retained host for each generation M1, M2, M3 and M4. All
four cells use the same SHA-256-sealed release artifact, Windows 11 ARM64 image
and UEFI vars source. Each row records the exact Mac model identifier, macOS
version/build and SHA-256 of its flat retained receipt.

A cell is clean only when all of these are directly observed as `yes`:

- a dedicated test user had no checkout/source tree;
- BridgeVM.app and BridgeVM user data did not exist before install;
- `BRIDGEVM_REPO_ROOT` and `BRIDGEVM_SWTPM_BIN` were unset;
- no PATH-selected helper was used;
- the release-object override scan passed;
- release-artifact install, Windows boot, update and rollback each passed.

The machine needs neither Homebrew nor a developer toolchain. Repository
scripts may orchestrate evidence externally, but the app under test must use
only the sealed release artifact and bundled helpers. Receipt fields are flat,
contain no paths/secrets, and are read with `O_NOFOLLOW` and a size bound.

The matrix TSV has fixed columns enforced by
`scripts/validate-clean-machine-matrix.py`; rows are ordered M1 through M4 and
must name four distinct `MacN,N` models. A fixed receipt directory contains
`M1.json`, `M2.json`, `M3.json`, and `M4.json`. The validator verifies every
receipt SHA and field against the matrix, requires identical release/image/vars
hashes, and fails if any cell or phase is missing. Current M4 evidence never
substitutes for M1–M3.
