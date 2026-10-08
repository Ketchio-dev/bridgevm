# Literal worker lock cleanup for custom queue paths — 2026-10-08

## Reproduced failure

The worker inserted its lock path into shell source for an EXIT trap. A private
queue name containing an apostrophe, already accepted by the queue/storage
boundary, broke that trap's quoting. An actual worker run against an owned
synthetic fence returned2 instead of126, reported an unmatched quote and left
worker.lock behind. The same fixture with a normal queue returned126 and removed
its lock. The fence bytes were unchanged in both cases. No installed worker or
live queue was invoked.

## Repair and focused evidence

The EXIT handler is now static shell text; it expands the canonical global
QUEUE_ROOT inside double quotes when invoked. Path characters are data, not
reparsed shell source. QUEUE_ROOT is established before acquire_lock and is not
reassigned while the worker holds the lock. This avoids relying on a local
variable after acquire_lock returns.

The actual worker regression exercises five path spellings (spaces, apostrophe,
double quote, dollar/command-substitution text and semicolon text), each with a
fence and an empty queue. It checks expected126/0 status, absence of the literal
lock, unchanged fence/sibling bytes and absence of an injected marker. Initial
fixture expectations omitted the empty CLI's job-ledger directory; that failure
was retained and the fixture corrected to assert the real empty-queue layout.

Focused regression passes ten subcases. The test is reachable from the existing
live-gate policy suite. Existing worker budget decreases; no ceiling increases.
This change does not add hostile-host lock-replacement protection or alter stale
lock policy. Exact full local and hosted checks must validate the sealed head
before integration; focused proof alone is not merge or live evidence.

No live service configuration, queue/fence, guest media, TCC or user asset was
changed. Product state and criterion statuses remain unchanged. A11 remains OPEN.
