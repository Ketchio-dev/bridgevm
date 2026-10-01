# A9 r56 first Studio pilot — failed physical diagnostic

Classification: one failed, nonpromoting physical diagnostic. It is also the
first job on the Mac Studio's live queue since that queue was reinstalled. The
current A9 criterion, defect wording and product state come from
`capabilities/windows-hvf.json`. A9 remains OPEN and the product remains
ENGINEERING_PREVIEW.

## Sealed observation

Physical job `t17-56e41dbd-studio-first-pilot-r56` ran main
`56e41dbd65e2c3221d94e37e0a8bd7af41c9b527` on the Studio: Mac16,9, macOS 26.7.

- The app was an exact-source build signed with the same Apple Development
  identity as the MacBook pilots, and classified as development-signed.
- Accessibility was granted to the helper once. The helper's own diagnostic
  reported it trusted, and the LaunchServices preflight passed.
- The T17 private inputs matched the MacBook's sealed rows by SHA-256.
- The input manifest SHA-256 began `8850771c`.

The job ran from 17:56:33 to 18:05:38 UTC and failed with
`product-model-failed`. Byte-identical receipts (SHA-256 beginning
`5da9b2b6`) passed the strict read check, and worker cleanup was verified. All
19 half-minute samples found the console unlocked. The product app was
frontmost from 17:57:08 until the job ended; Terminal was frontmost before the
app launched and after it closed.

Artifact preflight, VM creation, source preparation, Windows installation and
Secure Boot provisioning passed, in about 8 minutes. The lane then failed
before first READY with
`ax_tree_read_failed;attribute=AXWindows;role=AXApplication;ax_error=-25204;stage=identifier-search`.
The helper was looking for the runtime start-failure and state elements. Error
-25204 (`kAXErrorCannotComplete`) means the app did not answer an
Accessibility request in time.

## Reading

This is a different failure from the MacBook's click loss in r54 and r55. On
the MacBook, the same stage has passed in every pilot since r37. Whether the
app's main thread was busy at that moment, or the Studio's macOS 26.7 behaves
differently, cannot be told from this lane: the helper log is empty and no
first-READY packet applies. The run does establish that the Studio queue,
signing, Accessibility grant and private inputs work end to end.

## Follow-up

The pending display-click diagnostic (PR #292) does not cover this stage. A
transient -25204 at identifier search may need the same bounded retry that
other AX reads have. That has not been changed. A9 and A11 remain OPEN.
