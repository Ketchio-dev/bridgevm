# Reliability integration checkpoint — 2026-10-14

Evidence rank: **test**. Source `587bbb913c5fe3a980ca4adaa6d1d37cdf62153f` combines independently reviewed
repairs. The focus counts below overlap existing suites and must not be added
into a unique test total. Full local and exact-SHA GitHub-hosted verification
remain required for the resulting candidate. No live criterion is closed.

## Reproduced behavior and repairs

- Creation templates: an empty template selection disabled Create without
  presenting its prerequisite. The composed footer now shows the existing
  guidance while preserving actual errors, busy state and native-HVF modes.
  Native17 and actual-shim5 cases passed after6 failing baseline assertions.
- Install metadata: missing or rejected install requests previously produced
  an idle blank installer. Bounded request lookup now fails preparation with
  retry; a repaired request reuses the handle and stale work cannot publish.
  Native90 and actual-shim4 cases passed.
- Clipboard preference: an imported false preference was discarded by the
  codec and launch builders. The stopped library VM now saves that choice,
  retains its session/view identity and unsaved options, and distinguishes
  unpublished writes from published but unconfirmed durability. Legacy
  configurations default on. Native255 and actual-shim15 cases passed.
- xHCI: DNCTRL remained2 after Stop and HCRST. Controller reset now restores0,
  with the previous input-reset sequence extracted unchanged. The baseline
  failed and the xHCI suite passed192 cases; no new contract deviation.
- Resources: editing the draft during Apply could report16GiB after saving8.
  Completion now reports the accepted allocation while retaining the new
  draft. Native29 and actual-shim2 cases passed using the actual writer.
- Landing: the Source keyboard focus outline was clipped at431px. Compact
  navigation spacing now fits. Twelve viewport widths and keyboard,
  disclosure and reduced-motion checks passed with exact CSS substitution.
- Artifact recording: the writer named a retired policy rather than the
  packaged transition policy. Both path constructors now record the actual
  file and digest. Positional, app-only and environment paths passed; missing
  optional policy remains explicit and required app/DMG absence still fails.
  This recorder correction does not establish Secure Boot enforcement,
  signing or notarization.

## Input compatibility

The first Shift+Enter/Space patch was rejected during review: an old resident
agent advertised the samev3 capability but rejected the new chord, poisoning
subsequent ordinary input. The replacement retains the exactv3 base and adds
a nonce-bound optional keyset extension. Unsupported new chords are refused
locally without failing the ordered stream; old hosts retain theirv3 behavior.
Native67, actual-shim41, Rust3 and optimized transport2 cases passed, alongside
dry-built guest requests and old/mixed/new resident compatibility checks.
See [the protocol contract](../../machine-contract/resident-input-keysets.md).
Insertion and capability receipts do not prove application consumption.

## Preservation at deletion and restore boundaries

A completed snapshot exported into another VM's private restore-staging name
was deleted by the next restore. Admission now preserves manifest-bearing or
unrecognized preexisting staging and reports an explicit move/remove action.
**A complete staging generation left by an earlier crash also requires that
explicit resolution.** Recognized incomplete manifestless copies remain
automatically reclaimable. Identity-bound current-attempt publication cleanup
preserves exchanged old generations. The initial repair broke two publication
retry controls; those failures were retained and ownership handoff repaired.
The final snapshot suite passed187 cases and one public API regression; actual
CLI checks preserved the foreign export through repeated refusals, then
restored successfully after explicitly moving it aside.

Supported bundle relocation also allowed one VM's files to live inside another
VM's library entry. All product recursive deletion paths now check other saved
references. Review additionally reproduced a case-insensitive alias to an
in-entry symlink whose external target hid the dependency. Filesystem-identity
ancestor checks preserve that path without conflating distinct case-sensitive
directories. Native77 cases, actual-shim7 cases and a real case-sensitive
APFS2-case control passed. This protects saved references in the same library; it is not a
cross-library or concurrent-writer lock.

## Limits

No canonical media, guest vars, vTPM, recovery material or title content is
part of this change or its deterministic receipts. Existing structural
ceilings were lowered or retained, and new files registered at measured size.
Guest assets retain CRLF. Product state, capability wording, criterion
thresholds and known defects are unchanged. A9, A11 and A19 remain OPEN;
actual guest behavior, clean-machine journeys and release eligibility require
their own evidence.
