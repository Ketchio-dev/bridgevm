# Snapshot replacement member ownership — 2026-10-08

Evidence: deterministic tiny-file tests, not live guest or power-loss evidence.

A cooperating export could replace a previous snapshot while another export
copied its disk and vars. The reader owned both source member leases, but the
replacement writer owned only an output-parent key. A nested public create at
the reader's DiskSynced checkpoint replaced its source: the output contained
`old-disk` and `new-vars`, and its freshly computed manifest verified. The
baseline test actually reproduced this mixed pair; it was not merely a race
hypothesis. Outputs used different parents so parent contention did not hide it.

Creation now retains destination and stale-staging disk/vars path+inode ownership
alongside the existing parent lease, before any staging reclamation. A leased
member refuses with WouldBlock without deleting old work. New staging reserves
its member path keys before copying either file. Ownership remains held through
publication and old-generation cleanup. Existing destination admission and final
publication re-admission remain, including empty/absent and invalid-path policy.

The claim is cooperative exclusion, not protection from arbitrary filesystem
writers or an actor creating new aliases during staging. Existing hardlink
aliases conflict through inode ownership. No new public API or guest contract.

Tests cover the real nested-create reproduction, old staging retained on member
contention, existing hardlink owner, leased staging and fresh staging path locks.
Deleting the destination-member acquisition reproduces the same manifest-valid
mixed pair; mutation removed. Focused snapshot163/0 and full HVF library1362/0
plus one existing ignore pass; Clippy warnings-denied passes.

Initial broader tests retained two sequential refusal-message failures (symlink,
then nested directory). Non-contention errors now use the existing left-intact
refusal wording; tests were not relaxed. Initial rustfmt reported module order;
formatted order now passes. First delegated runtime attempt was not executed due
workspace/storage constraints; root ran all reproductions on SSD. A static review
request disconnected without a conclusion; its successor completed no-finding.

Exact sealed-head full local and hosted checks remain required. A19/A11/product
state remain unchanged; no actual disk/vars, VM, queue, service or fence mutation.
