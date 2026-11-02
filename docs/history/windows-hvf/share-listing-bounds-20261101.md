# Bounded shared-folder service (2026-11-01)

A guest directory response previously expanded every row into entries,
sync maps and queued file work without a listing-level allocation bound.
A large background file queue also consumed the control-file queue budget
and could leave user input behind every queued file transfer.

Guest listings now accept at most1MiB decoded bytes and4096nonempty rows.
Malformed, duplicate, directory and refused-name rows count toward the row
limit. Encoded length is checked before base64 allocation. Decoded bytes
are bounded before UTF8 validation; invalid UTF8 is refused as a whole.

Refused snapshots finish their wire request, report a fixed reason/count
without guest names, and leave the previous sync state intact. They never
become partial or empty snapshots that imply deletion. A later valid
snapshot resumes normally, including a genuinely empty directory.

Pending clipboard, control and heartbeat requests retain their relative
order ahead of background share requests. Background operations keep their
relative order, and an in-flight transaction completes before another is
sent. The existing1024foreground queue budget excludes background file
work, so a share backlog alone cannot reject newly appended user input.

Focused parser boundaries, service recovery/data-preservation and request
ordering regressions cover these contracts. Existing clipboard/paste and
input protocol tests are included in the focused service run. Final exact
local, hosted and required PR checks remain required at this checkpoint.
No live Windows run, frame-rate improvement, criterion promotion or
product-state change is claimed by these synthetic tests.
