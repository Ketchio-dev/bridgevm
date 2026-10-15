# Preservation and interface checkpoint — 2026-10-14

Evidence rank: **test**. This is a second isolated integration based on
`659836bab6bee31c8e3085178ab285afd19fe746`. Focused counts overlap suites;
they are not a unique aggregate. No guest or release criterion is closed.

## Registration, key custody and relocation

A deterministic interleaving through the actual GUI factory and native CLI
creation APIs let the GUI replace a newly published same-ID registration.
The baseline failed three assertions in two tests; its ordinary control passed.
New-registration writers now use exclusive publication. Existing mutable
configuration updates still replace records normally. An actual collision
preserves prepared media even if the winning record moves away before the
losing call handles its result. Published but unconfirmed directory durability
also preserves media and continues to report uncertainty, not success.

Cleanup conservatively keeps entries with a registration file, directory or
symlink, including unreadable and dangling entries. This deliberate behavior
can leave unregistered prepared files requiring explicit resolution. Existing
tests expecting removal of an injected registration directory failed in the
first iteration; that failure is retained, preservation expectations are
explicit, and marker-free owned rollback has separate controls.

A follow-up import test found that retained encrypted-state bytes could lose
their imported key when cleanup refused. Its two-test baseline failed two
assertions in one method. Preparation now releases key custody only after
owned file removal is confirmed; uncertain cleanup preserves both. Tests use
an in-memory key store, synthetic encrypted state and tiny/sparse media,
never real Keychain entries or recovery secrets. Final native41 and actual
shim34 cases passed; all new tests were found in the shim manifest. Independent
review checked the final source, evidence hashes and actual file budgets.

A separate supported nested relocation left another saved VM pointing at the
old, now-absent path. The baseline failed seven assertions. Move admission now
checks other saved same-library configurations and pending install requests
against the actual source bundle before journaling or file mutation. References
to unaffected entry siblings remain allowed. Native39, related-boundary36 and
actual-shim32 cases passed. An earlier broader run lacked its required helper
environment and failed14 assertions; it is retained as a setup failure.
These guards are point-in-time checks, not cross-library or concurrent-writer
serialization.

## Input and device behavior

Legacy paste buffering admitted text that the transport could never replay.
When earlier input finished, that text failed and discarded following input.
Two of four baseline methods failed ten assertions. The same clipboard/HID
classification and command bound now run before holding text, preserving
already accepted input on refusal. Boundary controls still admit1979 Hangul
characters and8192 printable ASCII HID characters. Native49, actual-shim49,
standalone SwiftTesting6 and optimized transport2 cases passed. This proves
host routing, not guest application consumption.

HDA codec state change latched STATESTS but did not deliver an interrupt when
its matching WAKEEN, CIE and GIE were enabled. Three public PCI/MMIO baseline
cases failed and one control passed. The controller interrupt predicate now
includes enabled state changes; existing CORB/RIRB, masking and reset behavior
is retained. All51 HDA cases passed with independent review. This is spec
alignment, not actual Windows audio or host wake evidence.

## Archive publication and landing accessibility

Failed packaging previously removed or replaced an existing output; choosing
an output inside the input app could also damage that app. Packaging now
refuses existing paths and in-app destinations, builds and verifies in owned
sibling staging, and publishes with native exclusive rename using held
source/destination directory descriptors. Unsupported primitives fail closed.
An initial hardlink-only approach was rejected for avoidable format limits.

The unchanged baseline failed32 assertions. Five final methods cover35 cases,
including file/directory/symlink races, injected creation and verification
failure, retry, input preservation, tar executable mode and contained
symlinks. These use real file/tar operations and the Darwin publication
primitive, with stub signature/DMG tools. They do not prove real signing,
DMG mounting, Linux or exFAT behavior. Existing entitlement checks and the new
publication suite share the full project check's packaging step.

At200% computed font size, narrow landing layouts clipped navigation and
technical labels. Flexible wrapping and grid sizing now preserve those labels;
screenshot-caption colors also exceed the tested4.5:1 contrast threshold.
Thirteen browser layouts, six keyboard variants and115 actual Tab stops
passed, with disclosure and reduced-motion checks. Browser-toolbar zoom was
not established;200/400% zoom coverage is equivalent CSS viewport coverage.
A scrolled screenshot initially clipped the brand; it remains recorded, and
all captures were repeated with asserted scroll position zero. Exact CSS was
substituted in an isolated browser; the existing served preview is unchanged.

## Verification and unresolved recovery

The first candidate3588 retained its full documentation-classification failure
(44 of45 stages passed). Corrected6598 passed all45 local stages with source,
index and tracked bytes unchanged; log SHA-256:
`6c54eadf6e9c7aa2b0a8a075c8308894066bbac7932cc464ecdfb89ee80e1db3`.
Its exact hosted full/CI completion was still pending when this history was
written. This second integration requires its own full and exact hosted checks.

The prior complete-stage preservation change blocks ordinary retry after a
new interrupted restore, reproduced with tiny actual-CLI fixtures. Diagnostic
harness patches that explicitly move the stage aside are **not integrated**:
T22's ordinary packaged-CLI retry, stop points, four boots and sample rules
remain unchanged. A product ownership repair is under separate investigation.
Legacy/foreign/ambiguous complete stages must remain preserved. A19 stays OPEN.
Product state, capability wording, thresholds and known defects are unchanged.
