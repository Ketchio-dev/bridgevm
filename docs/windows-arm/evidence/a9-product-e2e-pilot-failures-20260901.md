# A9 packaged-product E2E pilot failures — 2026-09-01

This record retains two failed T17 pilot attempts. Neither attempt is release
evidence, and neither closes A9.

## Sealed inputs

- tested commit: `44e808e7e7178f673429a8ba2058d80b6f83aab4`
- product artifact SHA-256: `489b60340e375485bc660459912d01ef35fada5ecc705ed0a010c0cdddb9a8d6`
- input manifest SHA-256: `c21ea648daf2acc6077638be903ef20a99d7e177d7d7b9f49caabbae61af1aed`
- Windows ISO SHA-256: `638aa2c88e94385b00f4f178d071e3df0b7d9e335577a83bd533b7f2eb65adf0`
- host: Mac17,9, macOS 26.5

Private Windows media, guest payloads, UEFI variables and lane artifacts stay
outside git. The Studio-local queue retains the complete job directories.

## Attempt 1: resident-worker contract mismatch

Job `t17-44e808e7-pilot` stopped before the T17 helper ran. The resident worker
was still at `34043bdd470a4556feff558ce3e813a4acf737bd`, whose hard-coded sealed-tier
list did not include T17. It therefore omitted `--input-manifest`; the sealed
T17 dispatcher exited 2 and the queue emitted a fail-closed missing-tier
receipt. Public receipt SHA-256:
`c778d17bdac531060c59c0f18279785334482eb2cbd89ce5726a25900069f670`.

This is queue-dispatch failure evidence, not a Windows or product-path result.
The failed job and burned job id were retained rather than rewritten.

## Attempt 2: macOS Accessibility denial

After the resident worker was updated to the sealed commit, job
`t17-44e808e7-pilot-r2` reached the packaged `BridgeVMProductE2E` helper and
launched `BridgeVMControl`. The lane proved artifact preflight, then stopped
with `failure_code=accessibility-untrusted` before VM creation because macOS
had not granted Accessibility control to the ad-hoc product build.

The public receipt is schema-valid and reports `pass=false`,
`criterion_pass=false`, `capability_promotion=false`, `run_count=1`, zero
completed product stages, and verified worker cleanup. Its SHA-256 is
`68b598eefbc424bd8cb4ef8cc45804a8c9fb9c8c8ca8cee5700578a85fd97305`.
The private authenticated lane result is retained only in the local queue; its
SHA-256 is
`a665f373e9c37ee4dadb68766aed27e5c95120538257d7560749ea05f9068ee6`.

This attempt proves the sealed manifest reached the correct helper, the
packaged product launched, and cleanup completed. It does **not** prove VM
creation, Windows installation, first boot, input, clipboard, folder sharing,
network, audio, Secure Boot, shutdown or snapshot/restore.

## Next gate

Grant Accessibility only to the exact sealed product automation identity,
then submit a fresh burned job id with an exact-head packaged artifact. A pilot
pass remains a single live run and still cannot close a criterion requiring a
release campaign.
