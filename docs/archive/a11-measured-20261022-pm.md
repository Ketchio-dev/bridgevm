# A11 observations replaced later on 2026-10-22

Classification: historical evidence only. The [registry](../../capabilities/windows-hvf.json)
and [capability matrix](../windows-arm/capability-matrix.md) own current state.
This file preserves A11 notes that later checkpoints replaced, verbatim. Copying them
does not revalidate them, close A11 or establish a release result. Earlier observations
are in the [earlier archive for 2026-10-22](a11-measured-20261022.md), the
[archive for 2026-10-15 to 2026-10-21](a11-measured-20261015-20261021.md) and the
[archive through 2026-10-14](a11-measured-through-20261014.md).

## T19 navigation seal

Seal `79deab41` recorded this note for source `89ed1524`, and `4a1753c7` dated it at its
actual seal time. The next #424 seal replaced it; it is preserved here.

- Source commit: `4a1753c79503792f963243e9849bf1889782a820`.
- Source field: `capabilities/windows-hvf.json`, criterion `A11`, `measured`.
- Source registry SHA-256: `e0b7e779c51845a6fb251c1664a9f0fb65f711c11979a26de22aba1312115c25`.
- Decoded UTF-8 measurement SHA-256: `f8b2ade9fb3c7923315e0ba8974a01903f7f7838aac130ffa66fb36ca1a31ad6`, 1263 bytes.

<!-- BEGIN ARCHIVED A11 MEASURED -->
2026-10-22 06:46 UTC checkpoint: A11 remains OPEN. Main 97de386c merged #420 through #423 on the operator's authorization; that exact head passed all 45 local stages, hosted full run 39714498832 and all 53 associated workflows. Source 89ed1524 adds a T19 import navigation change (26bfec47) and the 2026-10-22 live pilot records: an imported VM opens on its dashboard, and the import helper now presses the dashboard control before requiring runtime controls, as T17 does after install. A fixture modelling that dashboard failed one of three tests with the old wait and passes with the change; 366 focused product E2E tests pass. Exact local and hosted checks for this source are pending. These are deterministic checks, not guest behavior; the change is not live-validated because T19 r2 still failed before the dashboard. No final release regression seal is established. See the [A9 pilot record](../history/windows-hvf/a9-r82-r84-and-t19-main-pilots-20261022.md), [native UI diagnostic record](../history/windows-hvf/app-ui-417-live-diagnostic-20261022.md), [replaced A11 notes](../archive/a11-measured-20261022.md), [earlier replaced notes](../archive/a11-measured-20261015-20261021.md) and [verbatim A11 history](../archive/a11-measured-through-20261014.md).
<!-- END ARCHIVED A11 MEASURED -->
