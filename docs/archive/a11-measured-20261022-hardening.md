# A11 observations replaced by the hardening seal on 2026-10-22

Classification: historical evidence only. The [registry](../../capabilities/windows-hvf.json)
and [capability matrix](../windows-arm/capability-matrix.md) own current state.
This file preserves A11 notes that later checkpoints replaced, verbatim. Copying them
does not revalidate them, close A11 or establish a release result. Earlier observations
are in the [later archive for 2026-10-22](a11-measured-20261022-pm.md), the
[earlier archive for 2026-10-22](a11-measured-20261022.md), the
[archive for 2026-10-15 to 2026-10-21](a11-measured-20261015-20261021.md) and the
[archive through 2026-10-14](a11-measured-through-20261014.md).

## T19 r3 to r5 seal

Seal `a55563b1` recorded this note for source `afacf902`. The stacked hardening seal
replaced it; it is preserved here.

- Source commit: `a55563b1a30e19734061fb972a42ba98222e110c`.
- Source field: `capabilities/windows-hvf.json`, criterion `A11`, `measured`.
- Source registry SHA-256: `cb465b2260bcfb1fa2b41c76b301dafe93207e8a41a12fcdfcb5c408dbfe3769`.
- Decoded UTF-8 measurement SHA-256: `8f248bcbc5f420675d10e0f8ee8fa8c0f24ac33ac292ccf77421c8e0a1945603`, 1579 bytes.

<!-- BEGIN ARCHIVED A11 MEASURED -->
2026-10-22 15:13 UTC checkpoint: A11 remains OPEN. Main 97de386c passed all 45 local stages, hosted full run 39714498832 and all 53 associated workflows. PR #424's earlier head 4a1753c7 passed all 45 local stages, hosted full run 39736169846 and all 100 associated workflows. Source afacf902 adds six test-first changes to the T19 harness and the import path, with their records: the import name is entered through focus and confirmed; cleanup forgets the imported key of every lane registration against the sealed source state; the import end state is reported in fixed tokens; imported disk and vars become owner-writable; and the first boot also accepts the proactive PONG. Each change has a test that failed before it; the product E2E shim suite passes 397/0 and the focused media tests 4/0. Exact local and hosted checks for this source are pending. These are deterministic checks, not guest behavior; T19 r5 at 5f6c11a6 passed as one nonpromoting live pilot. No final release regression seal is established. See the [T19 record](../history/windows-hvf/t19-import-name-and-key-cleanup-20261022.md), [A9 pilot record](../history/windows-hvf/a9-r82-r84-and-t19-main-pilots-20261022.md), [native UI diagnostic record](../history/windows-hvf/app-ui-417-live-diagnostic-20261022.md), [replaced A11 notes](../archive/a11-measured-20261022-pm.md), [earlier replaced notes for 2026-10-22](../archive/a11-measured-20261022.md), [notes for 2026-10-15 to 2026-10-21](../archive/a11-measured-20261015-20261021.md) and [verbatim A11 history](../archive/a11-measured-through-20261014.md).
<!-- END ARCHIVED A11 MEASURED -->
