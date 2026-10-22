# A11 observations replaced by the review-fix seal on 2026-10-22

Classification: historical evidence only. The [registry](../../capabilities/windows-hvf.json)
and [capability matrix](../windows-arm/capability-matrix.md) own current state.
This file preserves A11 notes that later checkpoints replaced, verbatim. Copying them
does not revalidate them, close A11 or establish a release result. Earlier observations
are in the [hardening archive](a11-measured-20261022-hardening.md), the
[later archive for 2026-10-22](a11-measured-20261022-pm.md), the
[earlier archive for 2026-10-22](a11-measured-20261022.md), the
[archive for 2026-10-15 to 2026-10-21](a11-measured-20261015-20261021.md) and the
[archive through 2026-10-14](a11-measured-through-20261014.md).

## Hardening seal

Seal `0064eb05` recorded this note for source `29a7f1ac`. The review-fix seal
replaced it; it is preserved here.

- Source commit: `0064eb05d6379f095c7ad24b7e923798fcd2d6bc`.
- Source field: `capabilities/windows-hvf.json`, criterion `A11`, `measured`.
- Source registry SHA-256: `98a0b8e00216dfe102c318099765e9779f383c8640d4e7f994e877376fc7afeb`.
- Decoded UTF-8 measurement SHA-256: `958b23452ef9f729f99ce490de66a1063b52b29a8aa5a3cac856b7ed8b22f9ec`, 1729 bytes.

<!-- BEGIN ARCHIVED A11 MEASURED -->
2026-10-22 15:19 UTC checkpoint: A11 remains OPEN. Main 97de386c passed all 45 local stages, hosted full run 39714498832 and all 53 associated workflows. PR #424's earlier head 4a1753c7 passed all 45 local stages, hosted full run 39736169846 and all 100 associated workflows; its head a55563b1, which adds the T19 fixes and records, has exact local and hosted checks pending. Source 29a7f1ac stacks on a55563b1 and adds three test-first robustness fixes with their record: T17 cleanup forgets each lane's vTPM key through the sealed app or leaves cleanup unverified; a guest PUTCHUNK acknowledgement can no longer overflow the chunk sequence; and guest share names longer than macOS paths allow are refused, which bounds the unseen-name check. Each fix has a test that failed before it; the T17 key suite passes 15 tests, the T17 live-tier smoke 51 checks and the probe example suite 630 tests. Exact local and hosted checks for this source are pending. These are deterministic checks, not guest behavior; no live run has exercised these changes. No final release regression seal is established. See the [hardening record](../history/windows-hvf/t17-key-forget-and-agent-channel-hardening-20261022.md), [T19 record](../history/windows-hvf/t19-import-name-and-key-cleanup-20261022.md), [A9 pilot record](../history/windows-hvf/a9-r82-r84-and-t19-main-pilots-20261022.md), [replaced A11 notes](../archive/a11-measured-20261022-hardening.md), [other notes replaced on 2026-10-22](../archive/a11-measured-20261022-pm.md) and [earlier ones](../archive/a11-measured-20261022.md), [notes for 2026-10-15 to 2026-10-21](../archive/a11-measured-20261015-20261021.md) and [verbatim A11 history](../archive/a11-measured-through-20261014.md).
<!-- END ARCHIVED A11 MEASURED -->
