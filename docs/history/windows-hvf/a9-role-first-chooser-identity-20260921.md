# A9 role-first chooser identity — 2026-09-21

Historical evidence record. The capability registry owns current wording and
status. This checkpoint does not close A9 or A11, product state remains
Engineering Preview, and 3D remains outside General Preview and v1.

## Physical r25 result

Exact-main commit `6f1e21627bcfda5393bdc29d8ac2a3429f1d1add` completed
all 45 hosted push workflows. Its exact-source Apple Development-signed app
passed deep strict signing, entitlement, nested-helper, dependency, notice,
immutable-input and LaunchServices Accessibility preflight checks. The sealed
input manifest SHA-256 was
`dbc167dc94f742a75088f18b826b1f227625199c23edcf060498ff52df5d8407`.

Physical pilot `t17-6f1e2162-advertised-attribute-r25` passed artifact
preflight, automated the UI frontend, created the exact VM, prepared the
source, completed Windows installation and provisioned Microsoft-only Secure
Boot with 3D disabled. It then failed before first READY/PONG while opening the
runtime host-share chooser. The exact private failure was
`input-selection-failed` at `stage=initial-panel-check`: an AXIdentifier read
returned cannot-complete error `-25200` after the bounded snapshot retries.

The authenticated lane verified cleanup. The outer public receipt retained
`product-model-failed`, `criterion_pass=false`, `claim_eligible=false`, and
`capability_promotion=false`. Strict public and private receipts have SHA-256
`0ceb1abf6aba93747ad744766a65da106d462a61b3867e8b773f3ec8553cb12b`.
The authenticated private lane result has SHA-256
`dd038ac100f13ea618bf5436d0a7856c7230aae611f3005425c0f2f47cc024d2`.
Private paths, media and guest state remain outside git.

## Bounded correction

Source `d7e95111221e252c929bd680b2bfe986a2a6386d` projects the AX
role before reading AXIdentifier during open-panel discovery. Nodes outside
`AXWindow`, `AXSheet` and `AXDialog` are ineligible and therefore never incur
an identifier read. Eligible nodes still require the exact `open-panel`
identifier. Role-read failures, eligible identifier-read failures and distinct
duplicate matches all remain failures.

Six focused contracts prove that ineligible nodes are skipped before their
identifier accessor, exact identity is still required, duplicate references
to one object are accepted, distinct matches are rejected and attributable
read failures propagate. The ProductE2E target compiled and structural budgets
passed with new files registered at their actual sizes.

## Limits

The r25 pilot is a failed live single run. It proves installation and Secure
Boot stages for that lane, but it does not prove first READY/PONG, a usable
Windows desktop, integration behavior, shutdown, clean-machine completion,
installed-disk import or A9. The correction is deterministic host evidence
until a newly signed exact-source artifact passes another physical pilot. No
threshold or pass condition changed, and T19 remains required.
