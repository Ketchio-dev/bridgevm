# Installed state survives committed cleanup failure — 2026-10-08

Evidence: deterministic synthetic finalization/reconciliation tests and static
review. No Windows installation, guest boot, actual media or power-loss proof.

Installation commits final disk, provisioned vars, receipt, completed request and
installPending=false before removing its transaction. Actual production removal
can delete its journal, then fail syncing the parent. Reconciliation previously
caught this like a precommit failure and persisted installPending=true. Root's
real reconciliation baseline (tiny recovery fixture, interrupted at configStaged)
injected only the parent sync after successful deletion: one test/six failures.
It observed returned/persisted pending state, a subsequent scan recreating the
pending request, and Recovery.inspect actually admitting a fresh installation.
The committed final disk/vars/request remained byte-exact. No installer retry
was run against those outputs.

Cleanup failure is now typed separately, carrying the verified completed config
and a fixed warning without the private underlying error. Reconciliation returns
that config and issue without saving pending state. Finalization/recovery callers
still report unclean completion through existing failure handling; they do not
invent a successful completion callback. The warning says not to reinstall.

Committed journals resume from authenticated final artifacts rather than already
disposable staging. Final disk/provisioned-vars hashes, receipt, request digest,
config identity/completed state and absence of a pending request are checked
before the cleanup-only catch. Corrupt final artifacts remain fail-closed. Ticketed
recovery reads the completed request only for a committed journal; earlier phases
retain their request source and accepted-plan checks. Existing lock safety stays.

Review found two gaps in the first repair. Verified config now returns through
resume/reconcile/recover, avoiding another fallible config read after journal
removal that could demote the VM. Any residual transaction entry or lookup error
other than absence blocks new writable runtime launch, without falsifying the
installed state. This prevents legitimate guest writes from invalidating retained
installation-time hashes before cleanup. Existing relocation checks remain. An
orphan directory without a journal stays blocked; automatic repair is not promised.

Root focused57/0 includes parent-sync/partial staging removal, committed recovery,
final disk/vars/receipt/request corruption, no-postcleanup-reload and journal-less
residue/actual launch-readiness consumer. Removing the typed catch reproduces
one test/six failures; restored broader selection175/0 passes. Exact sealed full
checks remain required. Two advisory
requests failed with provider502 before a conclusion; successful successors were
read-only reviews, not independent runtime proof. Worker authored baseline seams;
root reviewed, reproduced and repaired. No structural ceilings are raised.

No product state, release criterion, permission, installed worker or fence change.
