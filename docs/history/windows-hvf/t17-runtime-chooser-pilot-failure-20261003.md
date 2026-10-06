# T17 runtime chooser pilot failure — 2026-10-03

This is a failed development pilot, not an A9 or release pass.
The asynchronous runtime chooser path did not close the live defect.
The earlier failed `035a889b` pilot remains failed.

## Sealed source and one attempt

- Commit: `8a4a390d8b5a3c1d6f787b9bbd287064b1583a8c`.
- Tree object prefix: `99dca9b3ccd4` (full identity retained privately).
- Job: `codex-t17-8a4a390d-runtime-chooser-pilot-r1`.
- Tier: `t17-windows-hvf-product-e2e`; pilot mode, one development-signed run.
- Submission returned zero at 12:52:38 UTC; worker began at 12:52:44 UTC.
- Producer finished at 13:01:01 UTC; worker finished at 13:01:02 UTC.
- Public receipt: failed, valid, one attempt, zero passes, one failure.
- Aggregate failure code: `product-model-failed`.
- Authenticated lane failure code: `input-selection-failed`.

## Recorded product stages and failure boundary

The existing host stamp matches the retained lane result hash and nonce.
The original request bytes were removed by normal cleanup and were not retained;
its recorded stamp hash cannot be independently rehashed from those bytes.

The producer records these five stages true: artifact preflight, VM creation,
source preparation, Windows installation and Secure Boot provisioning.
The ten stages from first READY through second shutdown are false:
first READY, keyboard/pointer, clipboard, folder share, network, audio,
first shutdown, snapshot restore, second READY and second shutdown.
Public final-media and guest-evidence hashes remain absent.
Private hash fields do not establish that a completed selected pair survived.

The lane detail is `stage=selection-confirmation; timed out before chooser stage`;
its opening action reports `open_ax_result=0`.
At this source, the selection-confirmation entry check precedes panel dismissal
and exact selected-path readback. Those confirmation predicates were not run.
The preceding accept-selection call returned, but individual action durations
were not recorded. The result does not identify which earlier read or action
consumed the deadline, or establish an AX deadlock or guest stall.
Runtime integration selects the host share before starting the guest runtime.

## Observation, cleanup and missing raw evidence

The genuine observer retained twenty samples and exited zero at 13:01:15 UTC.
It recorded the app lifetime and terminal app absence, with no sticky refusal.
`complete_sampled_observation=true` is sampled host observation only;
`continuous_state_proven=false` and `criterion_pass=false` remain explicit.
The stopped app was not reopened for inspection.

The lane app cleanup and public worker cleanup flags are both true.
At 13:05:25 UTC, the queue had zero queued, zero running and twenty-nine done;
a single comm-field process snapshot had no matching product/helper/probe names.
At 13:06:08 UTC, the exact recorded work path was absent.
These are scoped observations, not continuous or all-system absence proofs.

No full native installer log was captured or retained. The prospective bounded
collector was not cleared before normal lane cleanup; there was no capture
attempt, pipeline pause, cleanup change or retry. The retained helper log is empty.
No successful T19 handoff or new T22-ready pair was produced.

## Separate deterministic source checks

The exact source has required hosted CI green: thirteen standard jobs,
one manual full check with forty-three executed PASS headers, and eleven
portable/security/T20 companion jobs. Each T20 run executes seventeen suites
and 114 tests. The local full check has forty-four PASS results.
The hosted Linux absence and three SHIM opt-in skips are unexecuted checks.
Optional CGL still records `CGLChoosePixelFormat ret=10002 npix=0` and exit 101;
the containing job's successful API status is not a graphics pass.
Original producer hosted/security CI flags remain false and are not rewritten.
These separate deterministic receipts do not override the failed live result.

## Retained hashes

- [Public receipt](../../windows-arm/evidence/t17-8a4a390d-runtime-chooser-pilot-failure-20261003.json): `6e09fc55349162cff9b51f3cdbc73aa95f1d281de40ccf7aa29d0ac77082e952`.
- Lane result: `074ce2f908e57a2a5765ae810b0edb09edf5f0f5a21ef2e819d7d69dfdc04420`.
- Host stamp: `bd0fbd6eaa7fa0bacf8133846428d6766383581c54cf29a6d168a113430735c9`.
- Private retention record: `31e6678f5e028ef9e7b3cf888b64f300fb5274ed778adf0e5e6b3a9519995524`.
- Root terminal review: `7edf9b56d02bef8c1028d90698b72795aa76231641565cf0b310e7d3443888ca`.

Private media, requests, logs and machine paths are not copied into this history.
Capability states, fixed sample thresholds and release eligibility are unchanged.
