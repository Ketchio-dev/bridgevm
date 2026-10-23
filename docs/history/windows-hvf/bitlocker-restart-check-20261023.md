# BitLocker restart check: an opt-in T19 diagnostic — 2026-10-23

Classification: design and implementation record for an opt-in diagnostic that
the parent runs live later. The evidence is static reading of the code at
`8d3eb052` (rank 4) and, where a test is named, automated tests (rank 3). No live
run, Windows guest, BitLocker volume or VM ran for this work. No criterion,
threshold, stage, receipt field or product state changes, and nothing here is a
criterion pass.

## Question

The operator approved on 2026-10-23 a live vTPM PCR/BitLocker restart check. The
PCR part ran in [T17 r92](t17-r92-integration-pilot-20261023.md): PCR 0 and 7 were
identical at all three agent-ready boots, including a read after an in-boot guest
restart. No volume was encrypted, so no unseal was attempted. This record covers
the BitLocker part: enable BitLocker with a TPM-only protector, restart from
inside the guest, and see whether the next boot reaches the agent without the
recovery screen.

In product mode a guest restart ends the helper with exit 42 and the runtime
starts a new helper against the same swtpm, which sends one CMD_INIT before
`hv_vm_create` ([TPM backend failure](tpm-backend-failure-20261022.md)). A TPM
protector unseals only when that boot's PCRs were measured from reset, so the
boot after the restart exercises exactly that power cycle.

## Placement: the T19 helper, after its journey, before its final authentication

| Place | Decision | Why (static reading) |
| --- | --- | --- |
| inside the shared journey | refused | boot 2 is the mutation boot after the snapshot is created and boot 3 follows its restore ([which boots](t17-tpm-pcr-evidence-20261023.md)); encryption there changes what A9 measures |
| T17 | refused | an install is slower, and the T17 tier hands its final media to T19 as the next import source (`scripts/live-gates/retain-windows-import-source.py`), so an encrypted disk would become a T19 source |
| T19, after its last step | refused | after the helper exits, the tier's lane check re-hashes the selected final media and the final vTPM tree and re-reads the bundle's `logs/hvf/run.log` as the journey's final log (`scripts/live-gates/windows_product_e2e_guest_evidence.py`); a later boot changes all three and fails the lane |
| a second helper run after the lane check | refused | the helper's cleanup forgets the imported vTPM key (`apps/macos/Sources/BridgeVMProductE2E/A9ImportedKeyCleanup.swift`), so the VM cannot boot again without a new import |
| T19, after the journey, before final authentication | chosen | below |

- `A9ImportProductRunner.run` calls the check after `T17GuestJourney.run` returns
  and before `authenticateFinal`. All 14 A9 stages are then proven and the
  journey's last step has restored the snapshot, so the check boots that state.
- The check's boots are its own sequence: an enable boot, a guest restart, the
  unlock boot after it, and a clean shutdown.
- `authenticateFinal` then hashes the disk, vars and vTPM tree as the check left
  them, so the tier's re-hash compares the same state.
- The journey's final run log is moved aside before the check's first boot and
  put back unchanged after its last one. The check's own log stays in the lane
  bundle as metadata/product-e2e/bitlocker-run.log until the tier deletes the lane.
- If the journey failed, the check does not boot and its field says `not-run`.
- Consequence: the final media hashes of a check run describe the media after the
  check. Receipt keys and rules are unchanged, and the receipt's
  `input_manifest_sha256` binds the manifest that asked for the check. A check run
  is its own pilot and does not count toward A9.

## The switch: one row in the sealed T19 input manifest

- The T19 input manifest is the file given to `scripts/live-gates/bridgevm-live`
  with `--input-manifest`; the queue copies it into the job and seals its digest.
- It may carry one more row: `bitlocker_restart_check`, a tab, `tpm-only`. The
  verifier (`scripts/live-gates/windows-import-product-e2e-manifest.py`) accepts it
  at most once, with exactly that value, and only beside `campaign_mode` `pilot`.
  A second row, another value or a release manifest is refused at preflight.
- Only then does verified-inputs.json gain `"bitlocker_restart_check": "tpm-only"`.
  The tier then passes the helper one more option, through the existing launcher:
  --bitlocker-restart-check, accepted only in import mode
  (`--windows-import-product-e2e`); the T17 mode refuses it.
- Without the row, verified-inputs.json, request.json, the helper's arguments, the
  lane result and the receipt are byte-for-byte as before. Contracts check this.
- Not a request key: the app's E2E admission
  (`apps/macos/Sources/BridgeVMWindowProtocol/ProductE2EWorkBoundary.swift`) and the
  tier's lane check both require the exact request key set, so a key would change
  the app and the receipt writer in both modes. Not an environment variable: it
  would be neither sealed nor visible. T17 cannot ask for it: its manifest verifier
  refuses unknown rows.

## Guest sequence

| Step | Host action | Deadline | A failure ends the check with |
| --- | --- | --- | --- |
| 1 | stage the two BitLocker guest assets in the share, press Start | READY within 600 s (the journey's boot deadline) | `boot-not-ready` |
| 2 | wait until the BitLocker launcher and workload and the TBS helper are synchronized | 60 s | `asset-missing` |
| 3 | launch `BitLockerEnable` through the BitLocker launcher | marker within 60 s | `launch-failed` |
| 4 | read the enable result | 120 s | a `result-` reason, or the guest's refusal |
| 5 | enter `shutdown.exe /r /t 0 /f` | SYSTEM_RESET record within 180 s | `restart-not-observed` |
| 6 | wait for READY after that record: the unlock boot | 600 s | `unlock-boot-not-ready` |
| 7 | launch `BitLockerStatus` and read its result | 60 s and 120 s | `launch-failed` or a `result-` reason |
| 8 | enter `shutdown.exe /s /t 0 /f` | SYSTEM_OFF within 180 s | a product stop instead |

- After a refusal or failure the check goes straight to the stop. With the guest
  up, the stop is step 8. Without a READY or a SYSTEM_OFF it presses the product's
  stop control and waits up to 240 s until no process names the lane's disk path,
  the observation the app itself uses (`Shell.isProcessRunning`, `pgrep -f`).
  In `HvfEngineSession` an unanswered graceful stop ends the wrapper after 180 s.
- Step 5 is the guest-initiated restart, in product mode the exit-42 respawn. The
  host also records whether the run log holds `TPM2 TIS backend: power-cycled with
  CMD_INIT before boot` after the reset record.

The guest workload uses `Win32_EncryptableVolume` through CIM only:

- Called: `GetConversionStatus`, `GetProtectionStatus`, `GetLockStatus`,
  `GetKeyProtectors`, `GetKeyProtectorType`,
  `GetKeyProtectorPlatformValidationProfile`, `ProtectKeyWithTPM`, `Encrypt`.
- Never named: `GetKeyProtectorNumericalPassword`, `GetKeyPackage`,
  `ProtectKeyWithNumericalPassword`, `BackupRecoveryInformationToActiveDirectory`,
  `Get-BitLockerVolume`, `Enable-BitLocker`, `Add-BitLockerKeyProtector` or
  `manage-bde`. A contract checks the script against both lists.
- Why not the cmdlets: `Get-BitLockerVolume`, whose object `Enable-BitLocker`
  also returns, fills `KeyProtector.RecoveryPassword` for every numerical-password
  protector (recalled from the BitLocker module, not re-read), so even a status
  read would read a recovery password if one existed. `manage-bde` drives the same
  WMI provider.
- Enable, in order: refuse a Home edition (`EditionID` starting `Core`); require
  `Get-Tpm` present and ready; find the system drive's volume; require it fully
  decrypted with no key protector at all, so no recovery protector exists to be
  read; `ProtectKeyWithTPM` without a profile, the Windows default that
  `Enable-BitLocker -TpmProtector` also uses; require exactly one protector, of
  type TPM; `Encrypt` with `EncryptionFlags` 1, data only, which is
  `-UsedSpaceOnly`, and `Encrypt` rather than `EncryptAfterHardwareTest`, which is
  `-SkipHardwareTest`. Then it reads the state and PCR 0 and 7.
- Status: reads the same state and PCRs and changes nothing.
- No fallback: it never adds another protector, never retries, never suspends or
  decrypts. Any refusal is a fixed code and the check stops.
- The PCR hashes come from the existing TBS helper `bv-product-e2e-tpm.cs`, which
  the lane already stages; the values never leave the guest process.

## Guest result

The result is `t19-bitlocker-<phase>-<nonce prefix>.txt`: at most 512 bytes in
twelve LF lines, staged outside the share and renamed whole. The workload never
writes the shared T17 error file.

```text
bridgevm-t19-bitlocker-v1
nonce=<64 hex>
phase=enable|status
outcome=ok|<guest refusal>
edition=home|pro|enterprise|education|server|other|unknown
volume=<conversion 0-5>,<protection 0-2>,<lock 0-1>,<percent 0-100>|unknown
protectors=none|<ascending protector type codes 0-10>|unknown
pcr_profile=none|<ascending PCR indices 0-23>|unknown
fve_code=none|<0-65535>
pcr_read=ok|<TBS helper code>
pcr0_sha256=<64 hex>|absent
pcr7_sha256=<64 hex>|absent
```

- The longest legal result is under 512 bytes.
- `fve_code` is the low 16 bits of a failing method's return value when that value
  is an FVE error (0x8031xxxx), otherwise `none`.
- The host refuses anything else as `result-malformed`: another nonce or phase, a
  value outside its set, an unsorted or repeated list, a CR, a missing or extra
  line. A file that is not regular, is empty or is over 512 bytes is
  `result-unsafe`.

## Private field `bitlocker_restart_check`

It is written only when the check was requested, as a key of the private lane
result beside `tpm_evidence`:

```json
{"schema_version": "bridgevm.windows-product-e2e-bitlocker-restart-check.v1",
 "outcome": "unlocked", "reason": "none",
 "boots": [
  {"boot": "enable", "ready": true, "status": "available", "reason": "none",
   "edition": "pro", "conversion": "encrypting", "encryption_percent": 3,
   "protection": "off", "lock": "unlocked", "protectors": ["tpm"],
   "pcr_profile": [7, 11], "fve_code": null, "pcr_read": "ok",
   "pcr0_sha256": "<64 hex>", "pcr7_sha256": "<64 hex>"},
  {"boot": "unlock", "...": "the same 15 keys"}],
 "guest_restart_observed": true, "power_cycle_after_restart": true,
 "pcr0_equal": true, "pcr7_equal": true, "vm_stop": "guest-shutdown"}
```

- Exactly two boot records with the same 15 keys; a field not observed is `null`.
- `ready` says whether that boot reached READY within its deadline. For the
  unlock boot `false` is the recovery-screen signal.
- `status` is `available` only when the guest answered with outcome `ok`;
  otherwise `reason` holds one fixed code, and observed fields are kept.
- `conversion` is `decrypted`, `encrypted`, `encrypting`, `decrypting`,
  `encryption-paused` or `decryption-paused`; `protection` is `off`, `on` or
  `unknown`; `lock` is `unlocked` or `locked`.
- `protectors` names protector types (`tpm`, `numerical-password` and the other
  WMI types), never a protector's content. `pcr_profile` lists PCR indices.
- `guest_restart_observed` is the SYSTEM_RESET record after the restart command;
  `power_cycle_after_restart` is the CMD_INIT line after that record.
- `pcr0_equal` and `pcr7_equal` compare the two boots' hashes.
- `vm_stop` is `guest-shutdown`, `product-stop`, `unconfirmed` or `not-started`.
- Only booleans, enums, small integers and SHA-256 hashes of PCR values. The
  object is bounded and under 4 KiB encoded.

## Reasons

| reason | from | meaning |
| --- | --- | --- |
| `journey-incomplete` | host | the A9 journey failed, so the check did not boot |
| `log-hold-failed` | host | the journey's final run log could not be moved aside |
| `not-reached` | host | the check stopped before this boot |
| `boot-not-ready`, `asset-missing`, `launch-failed` | host | step 1, 2, 3 or 7 failed |
| `result-missing`, `result-unsafe`, `result-malformed` | host | no result, an unsafe one, or not the exact format |
| `restart-not-observed` | host | no SYSTEM_RESET record within 180 s of the restart command |
| `unlock-boot-not-ready` | host | no READY within 600 s after the reset record |
| `unlock-state-unexpected` | host | READY, but the status is not an unlocked volume under conversion with only a TPM protector |
| `edition-unsupported` | guest | a Home edition, which lacks BitLocker |
| `tpm-not-ready` | guest | `Get-Tpm` failed or reported the TPM absent or not ready |
| `bitlocker-unavailable` | guest | no `Win32_EncryptableVolume` for the system drive |
| `volume-not-decrypted`, `protectors-present` | guest | the volume was already converted or protected; nothing was changed |
| `protector-add-failed`, `protector-unexpected` | guest | `ProtectKeyWithTPM` failed, or the protectors are not exactly one TPM protector |
| `encrypt-failed` | guest | `Encrypt` failed, including a policy that demands a recovery protector |
| `query-failed`, `script-failed` | guest | a status method or the script itself failed |

## What the outcome means and how it is reported

| outcome | condition | reading |
| --- | --- | --- |
| `unlocked` | enable `ok`, restart observed, unlock boot READY, its status an unlocked volume under conversion with only a TPM protector | at the boot after a guest restart, an exit-42 respawn that power-cycles the vTPM, Windows unsealed a TPM-only protector without the recovery screen |
| `recovery-suspected` | enable `ok`, restart observed, no READY within 600 s | consistent with the recovery screen; a hang or a slow boot gives the same signal, and the framebuffer is not read |
| `refused` | the guest refused at the enable boot | says nothing about unsealing |
| `inconclusive` | any other failure after the first boot started | no verdict |
| `not-run` | the journey failed, or the log could not be moved aside | no boot |

- The check passes with `unlocked` and fails with `recovery-suspected`; the other
  outcomes are no verdict.
- `protection` is recorded but not judged: WMI reports protection off while a
  volume is not fully encrypted (recalled from the `Win32_EncryptableVolume`
  documentation, not re-read), and a used-space conversion may still run at the
  unlock boot. Unlocked, under conversion and TPM-only is what proves the unseal.
- Reporting: only the private field. The check never throws; no stage, failure
  code, blocker or public field depends on it. The T19 lane writer admits the key
  without judging it, like `tpm_evidence`; the T17 writer still refuses it, and no
  receipt copies it.

## Live run for the parent

1. Pick a retained T19 source whose edition has BitLocker: Pro, Enterprise or
   Education. See the first risk.
2. Copy its T19 input manifest, keep `campaign_mode` `pilot`, and append one row:
   `bitlocker_restart_check`, a tab, `tpm-only`. The app rows must name the app
   built at the commit under test, as for any T19 pilot.
3. Submit the T19 tier with that manifest:
   `scripts/live-gates/bridgevm-live submit t19-windows-hvf-import-product-e2e --sha <commit> --input-manifest <manifest> --job-id t19-<short sha>-bitlocker-restart-pilot-r1`.
   Keep `tpm` out of the job id: the redactor refuses it, as r92 showed.
4. Expect about 30 to 35 minutes: T19 r8 took 24 minutes
   ([post-merge record](post-merge-integration-20261023.md)) and the check adds
   two boots, normally 5 to 10 minutes. Its deadlines bound it at about 37 more
   minutes.
5. Read `bitlocker_restart_check` in `private/lane-1-result.json`: `outcome`,
   `reason`, both boot records, `guest_restart_observed`,
   `power_cycle_after_restart`. The receipt keeps its meaning: it may pass or fail
   on the A9 stages alone.
6. Record the run as a diagnostic pilot that is not A9 evidence. A
   `recovery-suspected` outcome is not a proven recovery screen: that needs a
   framebuffer read, which this check does not make.

## Risks

- **Edition.** The T17 install applies index 1 of the ISO's install image
  (`scripts/win-assets/bvinstall.cmd`), which is Home in the consumer
  multi-edition ISO (recalled). Earlier guests report `EditionID=Core`
  ([second Mac migration](../../windows-arm/evidence/second-mac-migration-20260723.md),
  [Secure Boot proof](../../windows-arm/evidence/sb-guest-proof-20260723.md)). A
  source from such an install stops at `edition-unsupported` within the first
  boot and answers nothing about unsealing.
- **A stuck guest can still fail the lane.** After `recovery-suspected` the check
  needs the product's stop. If the VM is still running 240 s later
  (`vm_stop: unconfirmed`), the lane's final authentication may fail. This is the
  only path by which the check can change a lane's outcome.
- **Final media.** The disk is left under conversion and the vTPM state has two
  more boots, so the lane's final hashes differ from a run without the check.
- **Privilege.** The workload runs with the agent's token, launched like the
  other workloads. r92's `Get-Tpm` success suggests it is elevated; if not, the
  guest reports `tpm-not-ready` or `bitlocker-unavailable`.
- **PCR profile.** Without usable Secure Boot binding the Windows default profile
  is PCR 0, 2, 4 and 11 rather than 7 and 11 (recalled). Either way it is
  recorded, and both depend on PCRs measured from reset.
- **Hosted Windows CI.** The Windows PowerShell job also reads the hosted runner's
  own BitLocker status, read-only, and accepts any fixed outcome, so it proves
  the call shapes run, not what a guest answers.

## Tests

| Step | Test | Before | After |
| --- | --- | --- | --- |
| guest assets | `tests/integration/t19-bitlocker-guest-contract.py` | the guest assets did not exist; 8 of 9 failed | 9 of 9 pass, including 20 scenario results read by an independent Python parser |
| guest assets | `tests/integration/t19-bitlocker-guest-contract.ps1` (pwsh 7.5.2); the Windows PowerShell job runs it through `tests/integration/windows-guest-pwsh-contracts.ps1` | the workload file could not be read | PASS; the Windows job has not run |
| host plumbing | the T19BitLocker suites of the ProductE2E shim suite | pending | pending |
| host plumbing | t19-bitlocker-switch-test.py and t19-bitlocker-private-lane-test.py | pending | pending |
| defaults | t19-bitlocker-default-unchanged-test.py | pending | pending |

## Not verified

- No Windows guest ran either workload. The CIM method names, argument names,
  return codes and the protector and conversion codes are from memory of
  Microsoft's documentation and are exercised only against scripted answers.
- Whether Windows accepts a TPM-only protector without a recovery protector on
  the guest's build and policy, and how long a used-space conversion takes.
- Whether the product's stop ends a guest stuck at the recovery screen within
  240 s.
- That the TPM2 TIS power-cycle line reaches the run log. The probe prints it to
  standard output, which the run log captures, as it does the stop records.
