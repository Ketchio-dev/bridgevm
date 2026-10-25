# Post-merge integration, round 3: d6 run 5 and the follow-up branches (2026-10-23)

Draft PR #428 (`agent/followups-r2-20261023`), continuing
[the search field record](d6-search-binding-20261023.md) and
[the integration record](post-merge-integration-20261023.md). Evidence ranks
follow AGENTS.md: *live single run*, *automated*, *static reasoning*. Nothing
here promotes a criterion; A11 stays OPEN.

## 1. d6 run 5 at seal `493e1bbe`: PASS (live single run)

- Job `bvm-493e1bbe-native-ui-v2-20261023-05`, tier `d6-app-ui-host-v2`,
  submitted 2026-10-23T21:15:54Z, ran 21:16:34–21:16:46Z; `result=pass`,
  `exit_code=0`. Input manifest SHA-256 `bf40f4a5…`.
- All 12 driver requests succeeded, 6 of them mutations, `axRestarts` 0;
  driver and host both report success.
- The search set filtered the list on the first `searchState` poll (Amber
  hidden, Indigo shown, field value "Indigo"); clearing restored both rows
  and an empty field.
- Receipt: `outcome=diagnostic-complete`, `failure_code=none`,
  `claim_eligible=false`, `criterion_pass=false`, `boots_attempted=0`,
  `worker_cleanup_verified=true`. The private receipt (`6b8740ae…`) and the
  public one (`a74cf09c…`) differ only by `driver_result_sha256`.
- Every tripwire stayed 0; model creations 0/2/2 (option B held); the
  window list was unchanged and no crash report appeared.
- It is the first d6 v2 run to complete. Runs since 2026-10-14 stopped at
  the welcome controls, after the import form, at the overview press
  (runs 1–3 of 2026-10-23) or at filtering (run 4). Run 5 supports the
  search field hypothesis of `d6-search-binding-20261023.md` once.
- Confounders recorded at submit: a worker's `swift-frontend` at about 25%
  CPU, another tool's virtual machine at about 11%, load 5.6, and the
  operator's browser window on screen.

## 2. Seal `493e1bbe` checks (automated)

- Local full: PASS, all 45 stages, 21:18:41–21:37:25Z, clean tree before and
  after (log SHA-256 `8694a751…`); shim suites 425 / 1465 (3 skipped) / 65 /
  436; only the seven expected A19 negative-test FAIL lines.
- Hosted full: run 40025164641 (job 120659411090) PASS, 44 stages,
  `checked_source_sha=493e1bbeb3804937b0047a457adc7e47860634e1` (log
  SHA-256 `127af917…`), the same seven lines.
- All 103 workflow runs for `493e1bbe` succeeded.

## 3. Guest-write follow-ups (`04860894` … `63ad6ce5`)

- Six commits from `agent/guest-write-followups-20261023` (written on
  `8d3eb052`), picked in order. The first conflicted only in
  `scripts/refactor-budgets.tsv` (both sides appended new rows; both kept),
  the last only in `docs/document-manifest.tsv` (two `@include` lines; both
  kept). See [the follow-ups record](guest-write-followups-20261023.md).
- On the integrated tree every check of the branch passes again: probe
  example 708, shim suites, Python, shell, documents, schemas, product E2E
  smoke, host-tail contracts; budgets hold at each picked commit with no
  raised ceiling, and the probe files gain no rustfmt hunk.
- T17's CLIPGET check is now an exact byte-length and SHA-256-prefix match
  where it was a CRLF-normalised substring test. The guest's own `Clipboard`
  workload already requires `Get-Clipboard -Raw` to equal the LF-terminated
  body, and passed in T17 r84, r91 and r92; after CLIPSET, bvagent serves
  the text it was given from its cache (*static reasoning on live single
  runs*). No T17 has run with the new check yet.

## 4. Opt-in BitLocker restart check (`7bea3568` … `2e8e88ba`)

- Four commits from `agent/bitlocker-restart-check-20261023` (written on
  `8d3eb052`), picked in order; only the first conflicted, in the budget and
  manifest files (appended rows and `@include` lines; both sides kept). See
  [the BitLocker record](bitlocker-restart-check-20261023.md).
- `2e8e88ba` closes a gap the branch reported: its guest workload contract
  (`t19-bitlocker-guest-contract.py`, 9 tests) ran in no runner. It now runs
  in `windows-import-product-e2e-contract-smoke.sh`, which passes.
- Only a sealed pilot row `bitlocker_restart_check<TAB>tpm-only` turns the
  check on. Without it the T19 verifier writes the same
  `verified-inputs.json` bytes as before, the lane launcher passes the
  helper exactly its old four arguments, and the T17 verifier refuses the
  row (*automated*). The guest workload only adds a TPM protector and
  starts a used-space conversion; it never creates, reads or stores a
  recovery password or key.
- **Edition boundary (static reasoning on repository evidence).** The
  installer applies image index 1 of the Windows image (`bvinstall.cmd`),
  and the installed guest reported `EditionID=Core`, Windows 11 Home, in
  [the 2026-07-23 migration record](../../windows-arm/evidence/second-mac-migration-20260723.md),
  where the BitLocker contrasts were already `BLOCKED_BY_EDITION`. Home
  does not offer BitLocker, so on today's sources the check can only stop
  at `edition-unsupported`. The install edition is not changed and no key
  is used here; a real BitLocker restart check needs a Pro, Enterprise or
  Education guest.


## 5. Integrated tree at `2e8e88ba` (automated)

- fmt; clippy for the workspace and with `venus`; probe example 708; HVF
  library 1483 (2 ignored); shim suites 430 / 1465 (3 skipped) / 65 / 454;
  the hand-listed Swift contracts outside `check-project.sh`; reset-record
  scan; Python, workflow, document and schema checks; product E2E smoke;
  selected-media, TPM PowerShell and host-tail contracts; budgets at every
  picked commit with no raised ceiling; `git diff --check`. The probe files
  gain no rustfmt hunk over `493e1bbe`.

## Not verified

- d6: one pass only; the in-process `AppUIHostLegacyScenario` and
  `HvfAppUIScenario` text-set paths were not run against the cell-carried
  identifier.
- The new CLIPGET comparison, the copy route, the refusal table and the
  0700 directories have not run against a live guest.
- The BitLocker check has not run live; its share re-push and power-cycle
  line after the guest restart come from reading the code. If both the
  guest shutdown and the product stop fail, the final media authentication
  could hash a running disk and fail the lane (only when the check is on).
