# T17 LaunchServices Accessibility preflight

Date: 2026-09-17

Source commit `4ab4b385729456a6acd03bfd1122a1cd1fa91d62` changes
only T17 admission tooling and its deterministic contracts. A9 remains OPEN
and 3D remains outside the release path.

The exact-main pilot `t17-36c21114-3d-off-pilot-r1` failed before VM
creation with `accessibility-untrusted`. A direct invocation of the exact
packaged helper had reported trusted, while an independent LaunchServices
invocation of the same helper reported untrusted. Both observations named
cdhash `66a2f64bb8c6f4e8e8240775090ae9add1d8219b`. The direct observation
therefore did not represent the process identity and launch mode used by T17.

The queue submission path now verifies the app tree and helper against the
T17 manifest, launches the helper app through `/usr/bin/open`, and strictly
validates the bounded observation-only diagnostic. Missing synthetic inputs
retain their existing worker-side failure receipts, and non-macOS checks do
not claim to observe TCC. An untrusted or malformed observation fails before
the queue directories or immutable job-id ledger are created.

The exact packaged app remained untrusted through LaunchServices after this
change. A direct submission test was refused and created neither queue nor
ledger state. No second live job was submitted. The external app preflight
record was corrected from trusted to untrusted rather than preserving the
earlier overclaim.

The focused product contract passed 44 T17 checks plus the installed-disk
import contracts. The complete project check passed: 989 HVF tests with one
intentional ignore, 381 probe tests, shim suites 425, 805 with two live-only
skips, and 62 tests, plus CLI, formatting, clippy, documentation, security and
structural budgets. These deterministic results do not prove Windows
installation, imported boot, guest shutdown, performance, or A9 completion.
