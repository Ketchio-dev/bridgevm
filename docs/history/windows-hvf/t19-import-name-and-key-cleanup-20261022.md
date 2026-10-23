# T19 import name and imported-key cleanup — 2026-10-22

Classification: diagnosis of four T19 physical pilot failures, the fixes,
and the first passing T19 pilot. The diagnosis rests on host observations of
live single runs and on static reasoning. The fixes are automated-test
evidence. r5's pass is a live single run, nonpromoting, with
`claim_eligible=false`. A9 remains OPEN and the product remains
ENGINEERING_PREVIEW.

This replaces the "cause unknown" conclusion for T19 r1 and r2 in
`a9-r82-r84-and-t19-main-pilots-20261022.md`. That record stays as written.

## What the host shows

Read after the runs from the Studio's unified log and from the item
attributes of the login keychain. No secret data was read.

- **r1** (`t19-97de386c-main-import-pilot-r1`), 05:47:43–44Z:
  - The app probed the data-protection keychain and got -34018, as every
    BridgeVM app without keychain entitlements does; T17 r84 shows the same.
  - It found no existing key, then updated and added a legacy-keychain item.
  - That item's account is `windows-11`, created 05:47:43Z. The requested
    slug was `bridgevm-a9-import-lane-1-2aff9357354f`. T17 r83's and r84's
    keys carry their requested lane slugs.
- **Cleanup:** no BridgeVMControl process made a keychain call at the end of
  r1 or r2, so forget-import never ran. Both receipts still say
  `cleanup_verified=true`.
- **r2** (`t19-26bfec47-import-dashboard-pilot-r2`), 06:22:19Z:
  - The app made the same probe and a legacy read. securityd then decoded
    the keychain and checked the client's code requirement. In r1 and
    T17 r84 it did that only when writing.
  - The app made no further keychain call in the remaining 19 minutes.
- **Confirmation, 13:22:08Z:** the r1 app build's `--vtpm-lifecycle
  forget-import --stable-vm-id windows-11`, given the T19 source's vTPM
  state, package and code, exited 0 with `forgot_imported_key=windows-11`.
  The command deletes a key only when it equals the key opened from the
  source recovery package. The `windows-11` key was therefore the T19
  source's imported identity.

## Reading

- r1 and r2 set the name with `setText` on the first-run import form's
  name field, which has no focus there. SwiftUI binds a value set through
  accessibility only while the field has focus (`T17TextEntry.fill`).
  T17's identical call works because the create sheet focuses its field.
  Both imports used the form's default name "Windows 11", slug
  `windows-11`.
- r1's import got as far as installing its key. Whether it committed and
  showed the dashboard was not observed. If it did, r1 also lacked the
  dashboard navigation added in `26bfec47`, and its registration check
  would have failed on the slug.
- The cleanup looked only under the requested slug. It found nothing and
  reported success while the key stayed.
- r2's import read r1's leftover key. The product refuses an import whose
  destination key exists ("대상 VM ID에 이미 vTPM 키가 있습니다"). Whether r2
  stopped on that message or on a keychain prompt was not observed. Either
  way it could not reach the dashboard.

## Fixes in PR #424 (test-first)

| Commit | Change | Before → after |
| --- | --- | --- |
| `51410503` | Sets the name through focus (`fill`). Requires the field to show it after the inputs are chosen, else stops with `input-selection-failed`. | 2 of 3 new tests fail → pass |
| `195da6a4` | Cleanup forgets the key of every lane registration, whatever its slug. A refusal, a failed forget or an unlistable library leaves cleanup unverified. | 4 of 4 fail → pass |
| `83880807` | The import error text and the recover button get identifiers. A missing dashboard now reports the import stage, an error class, both buttons and the lane registrations as fixed tokens. | 1 of 5 fails → pass |
| `a8ebc676` | The imported disk and vars get mode 0600 after the native copy (see T19 r3 below). | 1 of 1 fails → pass |
| `5f6c11a6` | Cleanup opens the package against the sealed source state, not the lane's rewritten state (see T19 r4 below). | 2 of 4 fail → pass |
| `4b8f5bf7` | First boot also accepts the host's proactive PONG, as T17 and the receipt verifier do. No run has hit this. | 1 of 3 fails → pass |

The ProductE2E shim suite passes 397/0 at `4b8f5bf7`. The focused HvfMediaImport
tests pass 4/0 at `a8ebc676`, using the real native helper.

## T19 r3 and the imported media modes

T19 r3 (`t19-83880807-import-name-pilot-r3`) ran app build r86 at `83880807`
from 13:26:47Z to 13:31:43Z. Its input manifest SHA-256 begins `f3aa193d`.

- **Receipts:** identical, SHA-256 beginning `07031f60`. Cleanup was verified.
- **Stages passed:** artifact preflight, source authentication, UI import
  and imported media authentication. It is the first T19 run past the import.
- **Failure:** `identified UI element remained disabled:
  bridgevm.windows.runtime.start; timeout_s=20.0`.
- **Local experiment:** the r86 build's own `snapshot_pair_cli create` on
  0444 inputs wrote a 0444 `disk.raw` and `vars.fd`. The tier makes lane
  inputs read-only, and an APFS clone keeps the source's mode.
- **Reading:** the imported disk and vars stayed read-only, and launch
  readiness reported `target-disk-access` and `uefi-vars-access`, which
  disable Start. The new test reproduces both blockers with the real helper.
  The live effect is inferred, not observed.

## T19 r4: the whole guest journey, then cleanup refused

T19 r4 (`t19-a8ebc676-import-media-pilot-r4`) ran app build r87 at
`a8ebc676` from 13:54:08Z to 14:14:58Z. Its input manifest SHA-256 begins
`ee05ba02`.

- **Receipts:** identical, SHA-256 beginning `d8b368c8`. The worker's tree
  cleanup was verified.
- **Stages passed:** all fourteen, from artifact preflight through the
  second shutdown, including keyboard and pointer, network, clipboard,
  audio, folder share and snapshot restore. This is the first T19 run to
  complete the guest journey, and it confirms the media fix live.
- **Failure:** `cleanup-failed`. The helper's forget-import passed the
  lane's own vTPM state, which the guest boots had rewritten, so it no
  longer matched the package and the product refused. The imported key
  `bridgevm-a9-import-lane-1-e08c300b376e` (created 13:57:55Z) stayed.
  - A read-only review predicted this while r4 ran. r3's key was forgotten
    because its VM never booted.
- **Removal:** at 14:16:43Z the r87 build's forget-import, given the sealed
  source state, removed that key (exit 0).

## T19 r5: first passing T19 pilot

T19 r5 (`t19-5f6c11a6-import-cleanup-pilot-r5`) ran app build r88 at
`5f6c11a6` from 14:22:10Z to 14:46:38Z. Its input manifest SHA-256 begins
`3d1124f2`.

- **Result:** pass, failure code `none`. All fourteen stages passed and
  cleanup was verified. 3D injection was not used, as the 3D-off
  configuration requires.
- **Receipts:** identical, SHA-256 beginning `e84ac90b`. The strict import
  receipt verifier accepts both copies for commit `5f6c11a6`. With
  `--require-claim-eligible` it refuses, because an installed-disk receipt
  alone cannot close A9.
- **Console:** unlocked in all 53 samples, with no screen sharing and no
  fence.
- **Key:** afterwards the login keychain held no `bridgevm-a9-import-*`
  item. This host observation means the lane's own cleanup forgot its
  imported key.
- **Limits:** this is one nonpromoting pilot on the development Mac, from
  a source made by T17 r84 on the same Mac. It is not a clean-machine
  receipt. The first-boot change `4b8f5bf7` came after r5 and is not in it.

## Residue

- The `windows-11` key was removed at 13:22:08Z, and r4's import key at
  14:16:43Z, as above. r3's and r5's imports left no key.
- 16 items remain under `com.bridgevm.vtpm-state-key.v1`. 13 are T17 lane
  keys from 2026-10-01 and 2026-10-02, 2 are T17 r83's and r84's lane keys,
  and 1 is a name from 2026-08-07. None was touched. T17 never forgets its
  lane key; a separate fix is in progress.
