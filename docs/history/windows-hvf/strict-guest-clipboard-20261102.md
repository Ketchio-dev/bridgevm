# Strict guest clipboard decoding (2026-11-02)

Malformed base64 was converted into an empty response. Although that empty
response did not overwrite the Mac clipboard, it consumed a pending private
paste rebaseline. A later legacy paste could then be mistaken for a new guest
copy and published to the host. Invalid UTF-8 also became replacement text.

Both decode failures now refuse the entire response through the existing
unknown-baseline and refusal-fingerprint policy. A rejection preserves a
pending paste rebaseline. Later valid copies recover under the same ownership
rules. Diagnostics contain fixed reasons and counters, never clipboard bytes.

The existing 8 MiB decoded limit, encoded-size precheck and valid-empty ignore
policy remain. This is a text-decoding correction, not a new clipboard limit.

Before the fix, the privacy and invalid-UTF8 regressions failed while the
valid-empty case passed. After the fix, all 48 focused clipboard contracts
passed, including exact-8-MiB, one-byte-over, recopy, ownership and FIFO cases.
Production example clippy with warnings denied and structural budgets passed.
The tests use a fake clipboard and do not access the Mac pasteboard.

Exact final project, hosted and required PR gates remain pending. No live
clipboard or Windows criterion result is claimed; product state stays unchanged.
