# Share snapshot and GET admission (2026-11-02)

A listing with missing fields or an invalid size silently omitted that row.
The remaining partial snapshot could be interpreted as a deletion. All
nonempty rows are now checked before replacing the output or sync snapshot:
four fields, a u64 size and an explicit directory flag are required.
Whole-list byte/row limits, right-split names, opaque timestamps and name
refusals remain. A malformed snapshot keeps prior state; a valid empty
snapshot retains the existing deletion behavior.

Resident GET also consumed reassembly under the current request name without
checking its BEGIN path. Completion now requires both the active resident
request and its full guest path. A new resident GET and an error clear stale
reassembly. Mismatched completion cannot write, adopt a baseline or complete
the wire slot. Existing size and destination-failure fixtures now use actual
framed replies. This adds no unique request ID and cannot distinguish every
late complete frame for the same path.

The original seven focused regressions produced two passes and five failures:
an owned host file was deleted, partial work was queued, malformed metadata
was accepted, a different-path body overwrote the requested file, and a stale
same-name body completed a later request. The failure inputs are retained.
After repair, all 267 focused clipboard/share tests passed, with production
example clippy warnings denied and formatting passing.

These are synthetic parser/service results using temporary owned files, not
live Windows or operator shares. Exact final project, hosted and required PR
gates remain necessary. Criteria, limits and product state are unchanged.
