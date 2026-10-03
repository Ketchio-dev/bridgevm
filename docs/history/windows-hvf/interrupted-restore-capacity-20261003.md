# Reclaim interrupted restore staging before capacity admission — 2026-10-03

Classification: deterministic native fixtures and static independent review.
No real guest/media run or actual filesystem ENOSPC is established. A19's
fixed 10/10 campaign, known defect and product wording remain unchanged in
`capabilities/windows-hvf.json`; this result does not close the criterion.

## Reproduced failed retries

Frozen source `4ea7ca108465a015a9f9d6a49a2bf36f96ae165f` checked free capacity
before reclaiming owned staging left by an interrupted restore. The existing
native hard-exit child exited 71 before publication, leaving a real 16-byte
staged disk-plus-vars pair in a fully owned tiny fixture.
A controlled 16-byte provider reported zero available while those bytes
remained. Both original-selection and current-selection retries rejected
InsufficientSpace, needed 16/available 0, before reaching reclamation.
Two desired recovery assertions failed; the old selected pair remained intact.
Raw baseline log SHA-256:
`160971f9793c9632d5ac119b7fee5e5717babf26119f6ccd5967e1067c1b2bba`.
The provider models admission capacity; no host volume was filled and no
cross-volume Windows interruption, physical power loss or live recovery is
inferred from this test. Existing hard-exit tests lacked this capacity pressure;
ordinary streaming-ENOSPC tests reclaimed staging within the live process.

## Resulting admission behavior

Restore verifies the canonical external source and checked disk-plus-vars
length first. While retaining its pair lease, it validates the existing real
private managed root and private staging before reclaiming stale copies.
Capacity is then sampled using the unchanged size boundary. Missing managed
storage remains absent when admission refuses; initialization, staging and
publication retain their previous order afterward.

Invalid sources, root/staging symlink or permission changes refuse without
removing caller-owned sentinels. Continued insufficient space still refuses
and preserves selection. Equality and unavailable-provider behavior remain
unchanged. This adds no quota relaxation or new physical-loss guarantee.

## Focused conclusion

Retained development source SHA
`7927189ce0655cfe9a4d19d8e987ac413ef47a8e` is integrated as `b9289d49`.
Eight new native cases passed: authentic hard-exit/retry with original and
current selection, invalid source, unsafe root/staging and remaining capacity
refusals. Both retries measured stale 0/available 16 and recovered safely.
Raw eight-test log SHA-256:
`1a251af209688334f1e59544cbbf6ff98b6ab3ea9db15e7ae83ac13e863cecf9`.

The final snapshot-pair suite passed 152 tests, including existing selected
pair/ownership, real hard-exit and streaming-ENOSPC boundaries. Its raw SHA-256:
`873d3c3b42ddc0f0ca82a1c6e0731e7512f1b887dbdb7071d261338f7b578cfa`.
A19 contracts passed 12 suites/91 tests. Locked Rust 1.97 lib/test clippy,
formatting, reachability, whitespace and structural checks passed.
Nine source hashes stayed equal through final checks and matched the focused
commit; independent read-only source/log review found no required repair.
Four new modules register actual 20/10/65/214 lines with zero unsafe blocks;
existing admission ceiling decreases from 27 to 20. No ceiling increases.

Only owned synthetic files were used. Failed baseline and initial document
shard-header validation remain retained. Old samples or one-run receipts do
not replace the fixed campaign. Full integrated/project/hosted proof and a
matching artifact remain prerequisites before any hardware run of this source.
