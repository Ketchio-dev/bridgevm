# Ordinary HVF creation publication outcome — 2026-10-08

Evidence: deterministic synthetic creation/fault tests and static review, not a
Windows journey, GUI exercise, real guest media or power-loss evidence.

The registration writer already distinguishes pre-publication failure from a
successful rename followed by directory-sync failure. Ordinary Windows HVF
install/import factories collapsed both to false and removed their newly owned
bundle. A real install-factory baseline with separate library/storage roots
published vm.json, then injected sync failure after verifying renamed bytes:
one test had four failures because its bundle, staged ISO and request were gone.
The original tiny synthetic ISO was unchanged. This defect was reproduced, not
inferred merely from an error-return type. First-run import already handled it.

Only the two ordinary HVF factories now return a typed optional outcome. Genuine
notPublished remains nil and rolls back prepared files. Committed and nonpersistent
creation return created. PublishedButUnsynced retains registration and assets,
with fixed Korean warning text and no private exception details. Shared save and
isCommitted semantics are unchanged. Extracting install creation preserves its
signed-file-offset size guard before destination reservation.

The sheet does not automatically adopt or dismiss uncertain publication. It
retains the config and blocks another creation in both button eligibility and
the action itself. Explicit saved-VM readback/adoption does not rerun a factory
or re-save registration. Failed readback preserves both the original durability
warning and the new readback failure without appending duplicates. Successful
readback is not retroactive proof that directory sync succeeded. CLI uncertainty
throws before readback and success-output construction, identifying the saved ID.
Unchanged non-HVF factory results are mapped to created only at the caller.

Tests exercise real rename+injected sync failure for install and native-helper
RAW+vars import under default and separate storage, real rename-refusal rollback,
committed and nonpersistent creation, source preservation, warning privacy,
state/adoption decisions and CLI refusal. Sparse imports use bounded prefix/size
reads, never reading an expanded disk into memory. No GUI-hosted flow was run.

Initial focused run had five import failures because the real helper was absent;
root built the exact helper and supplied BRIDGEVM_TEST_SNAPSHOT_HELPER, with no
test or fallback relaxation: focused23/0. Collapsing uncertain publication to nil
then fails all four postrename cases; mutation removed. Restored broader native
selection:287 tests/one existing skip/zero failures plus three protocol tests.
Static review found a lost warning on failed recovery; repaired and regression
covered. Code-only worker authored factory/tests; root reviewed and executed.

Structural ceilings are not raised. Full exact sealed local/hosted checks remain
required. No release, criterion, permission, installed service or fence change.
