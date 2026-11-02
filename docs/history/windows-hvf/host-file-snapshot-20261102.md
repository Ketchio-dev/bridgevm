# Coherent bounded host upload snapshots (2026-11-02)

The host upload reader returned bytes and then looked up mtime through the
path independently. In the equal-hash branch, synced A could be read just
before same-size B atomically replaced its path. A's hash was then labelled
with B's mtime without PUT, suppressing B until another size/mtime change.
Different-hash PUT normally self-rechecks; the permanent-loss claim is narrower.

Uploads now read bounded bytes and metadata from the same checked regular-file
descriptor, opened relative to the selected root. Existing component/leaf
NOFOLLOW and nonblocking regular-file checks prevent following a replacement
symlink; normal hardlink reads remain allowed. Observed size/high-resolution
mtime/ctime changes during the read refuse the version for retry.

Metadata preallocation, cap prechecks and cap+1 growth reads remain. The
existing 512 MiB cap/4 KiB allocation fixture was moved, with its assertions
unchanged. No unsafe or structural ceiling was added or raised.

The original service run logged 127 passes and one actual equal-hash failure.
A zsh readonly-variable bookkeeping error lost its Cargo exit capture; a
single compiled causal replay returned 101. Initial wiring also failed to
compile due to a duplicate re-export; those failures remain separate.
Corrected combined tests returned exit zero with 278 passes, including the
prior274 and four snapshot cases. CI1.97 clippy, formatting and budgets passed.

Only owned files and the existing modelled guest were used. This is not a
whole-filesystem atomic snapshot or a guarantee for edits preserving all
observed metadata. Exact final local, hosted and PR gates remain necessary;
no live guest criterion, limit or product-state change is claimed.
