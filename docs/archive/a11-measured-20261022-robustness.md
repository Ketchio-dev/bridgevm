# A11 observations replaced by the robustness seal on 2026-10-22

Classification: historical evidence only. The [registry](../../capabilities/windows-hvf.json)
and [capability matrix](../windows-arm/capability-matrix.md) own current state.
This file preserves A11 notes that later checkpoints replaced, verbatim. Copying them
does not revalidate them, close A11 or establish a release result. Earlier observations
are in the [review-fix archive](a11-measured-20261022-review.md), the
[hardening archive](a11-measured-20261022-hardening.md), the
[later archive for 2026-10-22](a11-measured-20261022-pm.md), the
[earlier archive for 2026-10-22](a11-measured-20261022.md), the
[archive for 2026-10-15 to 2026-10-21](a11-measured-20261015-20261021.md) and the
[archive through 2026-10-14](a11-measured-through-20261014.md).

## Review-fix seal

Seal `d1f17b2d` recorded this note for source `f330e32d`. The robustness seal
replaced it; it is preserved here.

- Source commit: `d1f17b2d58765045bf5b899bd1c182ad9f487a6f`.
- Source field: `capabilities/windows-hvf.json`, criterion `A11`, `measured`.
- Source registry SHA-256: `b5accee05a447ff776a31128c409700f81d3c24409217c1fad4cba364c159cbe`.
- Decoded UTF-8 measurement SHA-256: `ffc64e55cec6b84ed56701ec95593e713c7a873e277af36029b62e6e45ae8b32`, 1966 bytes.

<!-- BEGIN ARCHIVED A11 MEASURED -->
2026-10-22 17:30 UTC checkpoint: A11 remains OPEN. Main 97de386c passed all 45 local stages, hosted full run 39714498832 and all 53 associated workflows. PR #424's head a55563b1 passed all 45 local stages, hosted full run 39802858224 and all 101 associated workflows, and #425's earlier head 0064eb05 passed all 45 local stages, hosted full run 39803929983 and all 101 associated workflows. Source f330e32d stacks on 0064eb05 and adds test-first fixes from an independent review of both PRs, with their record: the T19 helper and the T19 tier each account for every lane's imported vTPM key and count only the product's exact proof or missing-key refusal; the T17 key tool tries every lane, removes its recovery material and prints no paths; and imported media become owner-writable inside the private import workspace. Each fix has a test that failed before it; the product E2E shim suite passes 403/0, the T19 key suites 15 and 4 tests with 6 tier cases, and the T17 suites 14 and 8 tests. Exact local and hosted checks for this source are pending. These are deterministic checks, not guest behavior; T19 r6 and r7 passed as nonpromoting live pilots of the T19 changes, and no live run has exercised the T17 or agent-channel changes. No final release regression seal is established. See the [review-fix record](../history/windows-hvf/key-cleanup-review-fixes-20261022.md), [hardening record](../history/windows-hvf/t17-key-forget-and-agent-channel-hardening-20261022.md), [T19 record](../history/windows-hvf/t19-import-name-and-key-cleanup-20261022.md), [replaced A11 notes](../archive/a11-measured-20261022-review.md), [other notes replaced on 2026-10-22](../archive/a11-measured-20261022-hardening.md), [more](../archive/a11-measured-20261022-pm.md) and [earlier ones](../archive/a11-measured-20261022.md), [notes for 2026-10-15 to 2026-10-21](../archive/a11-measured-20261015-20261021.md) and [verbatim A11 history](../archive/a11-measured-through-20261014.md).
<!-- END ARCHIVED A11 MEASURED -->
