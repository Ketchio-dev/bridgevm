# T17 guest TPM PCR evidence — 2026-10-23

Classification: new diagnostic evidence for the next live T17 or T19 run. The
evidence here is automated tests and static reasoning. No live run, Windows guest,
swtpm or VM ran for this change. No criterion, gate threshold or product state
changes, and nothing here is a criterion pass.

## Question

Since acae6e96 every product helper generation power-cycles swtpm with one CMD_INIT
before `hv_vm_create`
([TPM backend failure](tpm-backend-failure-20261022.md)). Before it, a product
guest restart (helper exit 42, a new helper against the same swtpm) kept the
previous boot's PCRs. No guest has shown which behaviour a Windows guest sees. This
change lets one live T17 run record it from inside the guest, at every boot at which
the agent is ready, without changing how T17 passes or fails.

## What a live run records (code; formats tested, rank 3)

At each recorded boot the journey launches `scripts/win-assets/bv-product-e2e-tpm.ps1`
through the product launcher (`bv-product-e2e-launch.ps1 -Action TpmBoot1..3`,
then `Invoke-CimMethod Win32_Process Create`), like the Audio workload, and waits
for its result file by name. The script and its Add-Type helper
`bv-product-e2e-tpm.cs` are copied from the packaged app into the lane share
beside the other product E2E assets.

The guest side:

- `Get-Tpm`: `TpmPresent` and `TpmReady`, or `unknown` when the cmdlet fails.
- TBS: `Tbsi_Context_Create` with `TBS_CONTEXT_PARAMS2` (version 2,
  `includeTpm20`), `Tbsip_Submit_Command` at locality 0 and normal priority (200),
  `Tbsip_Context_Close`.
- The command is TPM2_PCR_Read of PCR 0 and PCR 7 in the SHA-256 bank:

  ```text
  80 01         TPM_ST_NO_SESSIONS
  00 00 00 14   commandSize 20
  00 00 01 7e   TPM_CC_PCR_Read
  00 00 00 01   TPML_PCR_SELECTION count 1
  00 0b         TPM_ALG_SHA256
  03            sizeofSelect
  81 00 00      pcrSelect: bit 0 = PCR 0, bit 7 = PCR 7
  ```

- The helper accepts only the 96-byte success response: header, pcrUpdateCounter,
  the same selection echoed, two digests of 32 bytes. It returns the SHA-256 of
  each PCR value; the values never leave the guest process.
- The result, `t17-tpm-boot<N>-<nonce prefix>.txt`, is eight LF lines and at most
  512 bytes: a version line, the nonce, the boot number, `tpm_present`,
  `tpm_ready` (`true`, `false` or `unknown`), `pcr_read` (`ok` or a guest code),
  and `pcr0_sha256` and `pcr7_sha256` (a hash or `absent`). It is written outside
  the share and renamed in whole, and the script never writes the shared
  `t17-error` file.

The host side is the T17 helper (`BridgeVMProductE2E`). It adds one key,
`tpm_evidence`, to the private lane result `private/lane-N-result.json` of both
tiers:

```json
{"schema_version": "bridgevm.windows-product-e2e-tpm-evidence.v1",
 "boots": [
  {"boot": 1, "status": "available", "reason": "none", "tpm_present": true,
   "tpm_ready": true, "pcr0_sha256": "<64 hex>", "pcr7_sha256": "<64 hex>",
   "pcr0_equals_first_boot": true, "guest_restart_before_read": true},
  {"boot": 2, "...": "..."}, {"boot": 3, "...": "..."}]}
```

- Exactly three boots, each with the same nine keys. A field that was not observed
  is `null`.
- `pcr0_equals_first_boot` compares this boot's PCR 0 hash with boot 1's, and is
  `true` for boot 1 itself. It is `null` when either PCR 0 is unknown. The hashes
  also allow comparing boots 2 and 3 directly.
- `guest_restart_before_read` is the host's own observation, added beyond the
  requested fields because a PCR 0 comparison is uninterpretable without it. It is
  true when that boot's run log held a `stop: PSCI 0x84000009` record
  (`HvfStopLine.systemResetPrefix`; in product runs the exit-42 respawn) before
  the read, and `null` when the log could not be read whole (64 MiB bound).
- The host keeps only these hashes, booleans, boot numbers and fixed codes.
- `status` is `available` only when the guest answered with both hashes and both
  booleans. Otherwise it is `unavailable` with one fixed `reason`:

| reason | meaning |
| --- | --- |
| `not-reached` | the journey ended before this boot's read |
| `asset-missing` | the launcher, workload or helper did not reach the guest within 60 s |
| `launch-failed` | the agent did not confirm the launch within 60 s |
| `result-missing` | no result within 90 s of the launch |
| `result-unsafe` | the result is not a regular file, is empty or is over 512 bytes |
| `result-malformed` | the result is not the exact format, or names another nonce or boot |
| `tpm-query-failed` | Get-Tpm failed; the PCR hashes are kept when TBS answered |
| `tbs-context-failed`, `tbs-submit-failed` | the TBS call failed |
| `tpm-error-response` | the TPM answered with a non-zero response code |
| `pcr-selection-mismatch` | the response did not select PCR 0 and 7 of the SHA-256 bank |
| `pcr-response-malformed` | the response is not the exact success layout |
| `helper-failed` | the C# helper failed to compile or threw |

The public T17 and T19 receipts do not change. The receipt writers exclude
`tpm_evidence` (`scripts/live-gates/product_e2e_private_lane.py`) from the exact
key check that authenticates a lane, never read it, and still refuse every other
unknown key.

## Which boots, and which of them follow a guest restart

Static reading of `apps/macos/Sources/BridgeVMProductE2E/T17GuestJourney.swift`
(rank 4). T19, `A9ImportProductRunner`, runs the same journey, so it records the
same three boots.

| boot | journey point | read |
| --- | --- | --- |
| 1 | first READY after install (T17) or import (T19) | after the keyboard and pointer proof, whose form needs the foreground |
| 2 | after the snapshot is created (mutation boot) | right after READY |
| 3 | after the snapshot is restored (final boot) | right after READY |

- Each boot starts with the Start button, so it is a new runner execute with a
  new swtpm process (`runners/hvf-runner/src/launch_spec_execution.rs` starts
  one per execute and stops it when the run completes at SYSTEM_OFF). Between
  boots, swtpm is therefore always fresh. Only a guest restart within a boot, a
  helper respawn against the same swtpm, depends on CMD_INIT.
- Boot 1 of a T17 run is the first boot of an image applied offline, so specialize
  and OOBE run in it. The retained packet of
  [a9-r35](a9-r35-first-boot-oobe-20260929.md) (rank 2, one failed live run) shows
  three helper generations, two of them ended by guest restarts, in that window.
- Boots 2 and 3, and all T19 boots (an installed disk), are expected to reach READY
  without a restart (rank 4). `guest_restart_before_read` records what happened.

## How to read a run's record (rank 4 until a run exists)

Firmware measurements into PCR 0 come from the same firmware at every boot, so a
PCR 0 that starts from reset reads the same at every boot of a VM. Without a power
cycle, the firmware of a respawned generation extends a PCR 0 that already holds the
previous generation's measurements, and the PCR 0 at READY differs.

| boot 1 restart before read | PCR 0: boot 1 vs boots 2 and 3 | reading |
| --- | --- | --- |
| true | equal | the last respawn before READY started from a reset PCR 0, as CMD_INIT should give |
| true | differs, boots 2 and 3 equal | consistent with no power cycle at that respawn |
| true | differs, boots 2 and 3 differ | PCR 0 is not stable across boots here, so this method cannot answer |
| false | any | no restart preceded the read; the run says nothing about restarts |

An equal PCR 0 shows only that the last respawn before the read began from a
reset PCR 0. It cannot tell CMD_INIT from any other reset, and it says nothing
about earlier respawns or about a BitLocker or PPI flow.

## Pass and fail are unchanged (rank 3)

- The read never throws. `readTPM` and the asset staging are not declared
  `throws`, every failure becomes a fixed reason after a bounded wait, and the
  journey goes on to its next step.
- No stage proof, failure code, blocker or public field depends on it. Missing TPM
  assets in the app are skipped, not a lane failure.
- The guest workload writes only its own result, never the shared error file that
  later workload timeouts report.
- Bounds: the guest result is at most 512 bytes, each staged asset at most 64 KiB,
  and run-log reads at most 64 MiB. Each boot waits at most 60 s, 60 s and 90 s;
  the normal read takes seconds. The evidence object is three fixed records, under
  4 KiB encoded.

## Tests

Before means the test ran against the tree without the change it covers. Red and
green outputs were kept outside git.

| Commit | Test | Before | After |
| --- | --- | --- | --- |
| 04a58ec0 | `tests/integration/product-e2e-private-lane-contract.py` | both writers refused a lane carrying `tpm_evidence`, and a receipt built from it counted 0 passes instead of 1; 2 of 4 tests failed | 4 of 4 pass |
| 67527c16 | `tests/integration/t17-tpm-pcr-command-contract.py` | the guest assets did not exist; 6 of 6 failed | 6 of 6 pass, including responses built independently in Python and parsed by the guest helper under pwsh |
| 67527c16 | `tests/integration/t17-tpm-pcr-contract.ps1` (pwsh 7.5.2, C# 5) | Add-Type could not find the helper | PASS |
| b3495c04 | `T17TPMGuestResultTests`, `T17TPMEvidenceTests`, `T17TPMReadTests` (ProductE2E shim suite) | 13 failed against stubs, then the suite crashed on the stub's empty boot list | all pass |
| b3495c04 | `tests/integration/t17-tpm-journey-contract.py` | at 67527c16: 4 failures and 1 error of 3 tests | 3 of 3 pass |

- The Swift tests cover parsing (every malformed shape, every guest code, unknown
  booleans), bounds (513 bytes, empty, symlink, directory), PCR 0 equality,
  not-reached boots, the encoded keys and value types, the unavailable paths with
  their time bound, a partial result read again until whole, best-effort staging,
  and both private lane wrappers (the T17 lane gains exactly one key, 32 to 33).
- Two journey tests run the shared journey from first READY: the boot 1 read
  happens after the keyboard and pointer proof and the journey reaches its
  clipboard step both when the guest answers and when it never does.
- One existing test changed: `T17InputChallengeTests` passes the journey's new
  evidence log. Its assertions are unchanged.
- The shim suites, the Python script and shell checks, the structural budgets and
  the selected-media contracts pass.

## Not verified

- No Windows guest ran the workload. Get-Tpm under the agent's token, TBS command
  blocking, the guest's C# compiler and the time a read takes are untested. If
  the token is not elevated, Get-Tpm may fail and the record says
  `tpm-query-failed`.
- The helper compiled as C# 5 only under pwsh 7.5.2 with `/langversion:5`. The
  Windows PowerShell 5.1 job that also binds `tbs.dll`
  (`.github/workflows/t17-tpm-pcr-contract.yml`) runs only on hosted Windows and
  has not run.
- That boot 1 of a T17 run contains a guest restart rests on one failed run's
  packet; the record of the next run will show it.
- PCR 0 determinism across boots of one VM is static reasoning, and so is the
  reading table above.
