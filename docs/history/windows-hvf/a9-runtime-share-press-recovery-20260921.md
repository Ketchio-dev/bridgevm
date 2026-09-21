# A9 runtime share press recovery — 2026-09-21

Historical evidence record. The capability registry owns current wording and
status. This checkpoint does not close A9 or A11, product state remains
Engineering Preview, and 3D remains outside General Preview and v1.

## Physical r26 result

Exact-main commit `0d8b3e05ea8e9922536317a4721c563c832a8c45` completed
all 45 hosted push workflows, including core CI run `35584903236`. Its
exact-source Apple Development-signed app passed deep strict signing,
entitlement, nested-helper, wimlib, swtpm, dependency, notice, release-override
and LaunchServices Accessibility checks. The installed app tree SHA-256 was
`70041bc19896138b7e779634df8c58b9efb3454d019b64f750c49b4de067c353`,
the app executable SHA-256 was
`7505f47c282535f1333017ab1fceb570503d2fc76d524dd13efca3e78823670e`,
the product helper SHA-256 was
`5ce708f0fb1bb7ed664dc3a83ab5a5a390e0a938c0f8d6983c36f6f8fa48cd13`,
and the runner SHA-256 was
`92e7c466a7e1baa06b31ffc308e35cf07b6dc36cc8b83e53146a46ef1182fa27`.
The sealed input manifest SHA-256 was
`2393f29c9364a7ee73068fe426a4b541b3540af0ce36f4aa338aa665ab6b6d63`.

Physical pilot `t17-0d8b3e05-role-first-r26` passed artifact preflight,
automated the product frontend, created the exact VM, prepared the source,
completed Windows installation and provisioned Microsoft-only Secure Boot with
3D disabled. Live WinPE frames were observed during image application. The
lane authenticated its finalized 64 GiB disk, 64 MiB variables, Secure Boot
receipt and guest evidence. It then failed before first READY/PONG while
pressing the runtime host-share chooser button. Both exact AXPress attempts
returned `AXError.cannotComplete` (`-25204`) after successful activation with
the product frontmost:

`AXPress failed: bridgevm.runtime.share.host.choose; first_ax_error=-25204; retry_ax_error=-25204; activation_succeeded=true; frontmost=true`

The authenticated lane retained `failure_code=ui-element-missing` and
`cleanup_verified=true`. The outer receipt separately and truthfully retained
`failure_code=cleanup-failed` because its read-only payload left a 29 GiB
user-owned temporary root. After the job, that root was checked for ownership,
permissions, symlinks and related processes, its read-only directories were
made user-writable, and it was removed. The receipt was not rewritten.

The strict public receipt verified and has SHA-256
`81c1a6ee4f9b6162ecde660d52c37bcf69c6d5258ffdc22995dfaf6783903a09`.
The authenticated private lane has SHA-256
`08d2339e9c0f78cc95f3a951532f63d1b6deb66662fe629303b4d447c63ee0dc`,
and its lane result has SHA-256
`9941ef32c2a9dbd78a521fec990685c25c72055075d8286461297ea895258284`.
The public receipt itself records hosted CI as absent/false because that proof
was outside the worker boundary; the hosted result above is independent exact
commit evidence and is not attributed to the receipt. Private paths, media and
guest state remain outside git.

## Bounded correction

Source `a7c83b0222516be633a503efc069acd2571b7472` resolves press targets
by exact Accessibility identifier and exact `AXButton` role. When AXPress
returns only `cannotComplete`, it activates the application, waits 100 ms and
retries within the existing timeout. Missing or disabled controls,
attribute-read failures, activation failures and every other AX error fail
immediately. Diagnostics retain the first and retry AX codes, activation and
frontmost state, and attempt count.

All nine focused press-action tests passed. The complete ProductE2E suites
passed 141 XCTest and 41 Swift Testing contracts with zero failures. Structural
budgets passed at their existing ceilings; no ceiling was raised.

## Limits

The r26 pilot is a failed live single run. It proves installation and Secure
Boot stages for that lane, but it does not prove first READY/PONG, a usable
Windows desktop, folder or clipboard integration, shutdown, snapshot,
clean-machine completion, installed-disk import or release readiness. The
correction is deterministic host evidence until a newly signed exact-source
artifact passes another physical pilot. T19 remains required, and no threshold,
timeout or pass condition changed.
