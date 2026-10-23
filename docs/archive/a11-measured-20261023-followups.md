# A11 observations replaced by the follow-ups seal on 2026-10-23

Classification: historical evidence only. The [registry](../../capabilities/windows-hvf.json)
and [capability matrix](../windows-arm/capability-matrix.md) own current state.
This file preserves, verbatim, A11 notes that later checkpoints replaced. Copying them
does not revalidate them, close A11 or establish a release result. Earlier observations
are in the [robustness archive](a11-measured-20261022-robustness.md), the
[review-fix archive](a11-measured-20261022-review.md), the
[hardening archive](a11-measured-20261022-hardening.md), the
[later archive for 2026-10-22](a11-measured-20261022-pm.md), the
[earlier archive for 2026-10-22](a11-measured-20261022.md), the
[archive for 2026-10-15 to 2026-10-21](a11-measured-20261015-20261021.md) and the
[archive through 2026-10-14](a11-measured-through-20261014.md).

## Robustness seal

Seal `9fa07eb4` recorded this note for source `236032cb`. The follow-ups seal
replaced it; it is preserved here. That seal later passed all 45 local stages,
hosted full run 39885688347 and all 101 associated workflows.

- Source commit: `9fa07eb41a573a24d5829a04dba29cdce70bf2d4`.
- Source field: `capabilities/windows-hvf.json`, criterion `A11`, `measured`.
- Source registry SHA-256: `428921311bae0c9668c949ea6d24118f6ad14d662a50958c983d8a5bb3238df5`.
- Decoded UTF-8 measurement SHA-256: `cb0fa98acf5ac172467c8ae8ff0556ee6fbeebc6d26edc72625f07dd2801d8a6`, 2113 bytes.

<!-- BEGIN ARCHIVED A11 MEASURED -->
2026-10-23 01:42 UTC checkpoint: A11 remains OPEN. Main 97de386c passed all 45 local stages, hosted full run 39714498832 and all 53 associated workflows. PR #424's head a55563b1 passed all 45 local stages, hosted full run 39802858224 and all 101 associated workflows, and #425's head d1f17b2d passed all 45 local stages, hosted full run 39823341709 and all 101 associated workflows. Source 236032cb stacks on d1f17b2d and adds test-first fixes, each with its record: HVF session polls no longer republish unchanged state to every observer, the live display included; the native UI host diagnostic admits only inert-backend overview models, counted exactly, and its overview phase follows the current dashboard; a failed swtpm exchange fails closed instead of misaligning responses, a failed TPM reset stops the run instead of panicking, and each helper generation power-cycles the vTPM before its guest boots, which product guest reboots had not done since 2026-08-07. Each fix has a test that failed before it; the BridgeVMControl shim suite passes 1452 tests with 3 skipped, and a real-swtpm test shows a second helper generation booting from cleared PCRs. Exact local and hosted checks for this source are pending. These are deterministic checks, not guest behavior; no live run has exercised these changes, and whether the display stutter changes is not measured. No final release regression seal is established. See the [session record](../history/windows-hvf/session-heartbeat-republish-20261022.md), [d6 record](../history/windows-hvf/d6-overview-respec-20261022.md), [TPM record](../history/windows-hvf/tpm-backend-failure-20261022.md), [replaced A11 notes](../archive/a11-measured-20261022-robustness.md), [notes replaced earlier on 2026-10-22](../archive/a11-measured-20261022-review.md), [more](../archive/a11-measured-20261022-hardening.md), [more](../archive/a11-measured-20261022-pm.md) and [earlier ones](../archive/a11-measured-20261022.md), [notes for 2026-10-15 to 2026-10-21](../archive/a11-measured-20261015-20261021.md) and [verbatim A11 history](../archive/a11-measured-through-20261014.md).
<!-- END ARCHIVED A11 MEASURED -->
