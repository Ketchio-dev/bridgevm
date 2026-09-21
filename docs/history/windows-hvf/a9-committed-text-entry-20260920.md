# A9 committed runtime text entry — 2026-09-20

Historical evidence record. The capability registry owns current wording and
status. This checkpoint does not close A9 or A11, product state remains
Engineering Preview, and 3D remains outside General Preview and v1.

## Physical r20 result

Exact-main commit `5162c64312546514f1031964e1d22a242de06ab2` completed
all 45 hosted push workflows, including CI run `35554515463`. Its Apple
Development-signed, 3D-off artifact was used by physical pilot
`t17-5162c643-role-qualified-r20`.

The pilot passed artifact preflight, exact VM creation, source preparation,
Windows installation and Microsoft-only Secure Boot provisioning. The private
lane retained final disk, variables, installer-source, guest-evidence and
Secure Boot receipt hashes. It then failed before first READY/PONG with
`failure_code=guest-evidence-missing` and
`run_log=status=absent`. The public result remained `first-boot-failed`,
`first_ready_passes=0` and `criterion_pass=false`. Cleanup was verified and
3D injection remained false.

The strict public receipt passed repository verification and has SHA-256
`e76d451f4b0afe9ea6817b188ec119589ad97a10a916e355e8b455c889b6500b`.
The authenticated private lane result has SHA-256
`09de7972c7a7e15112abe39db0b2af305f0a30d7a427b15872494df1e839f9ca`.
Private paths and guest material are retained outside git.

## Measured UI state and correction

During the bounded first-ready wait, the exact host and guest share
`AXTextField` elements visibly contained the requested values, while Start
remained disabled and the app's readiness model reported both paths empty.
Both fields advertised the supported `AXConfirm` action. A helper process
sample located the wait in `T17ProductRunner.bootToFirstReady`, after
installation rather than in display rendering.

Source `5455b0cd91c5342dd97539d508a0fd802f0ab5bc` performs
`AXConfirm` after setting the exact role-qualified field, then reads the value
back and requires exact equality. Set, confirm and read failures all fail
closed. Eight focused role and text-entry contracts passed, including operation
ordering, refused confirmation and changed-value rejection. The complete local
metadata-tree project check passed with 7,725 retained log lines; its SHA-256 is
`f05e7853ac234ebecd1e27ff478acb89653e2e9edfcf62bd360ff61a21bd3a41`. PR 235's source-head hosted jobs
retained the expected stale-registry rejection while the observed independent
checks passed; exact metadata-head hosted verification remains required.

## Limits

This correction proves deterministic commit and readback behavior only. A new
exact-main signed physical T17 pilot must show whether the SwiftUI readiness
model commits the values and first boot launches. One successful pilot would
still be useful signal rather than the required release sample count, and T19
remains required. This record proves no READY/PONG, usable Windows desktop,
integration journey, regression closure, release readiness, performance or
graphics capability. No criterion, threshold or timeout changed.
