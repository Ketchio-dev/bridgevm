# Strict guest clipboard decoding (2026-11-02)

Malformed base64 became an empty response. It did not overwrite the Mac
clipboard, but consumed a pending private-paste rebaseline. A later value
could then be published as a guest copy. Invalid UTF-8 became replacement text.

Both decode failures now refuse the entire response through the existing
unknown-baseline and refusal-fingerprint policy. Rejection preserves a pending
rebaseline; later valid copies recover under the existing ownership rules.
Diagnostics contain fixed reasons and counters, never clipboard bytes.

The existing 8 MiB limit, encoded-size precheck and valid-empty ignore policy
remain. Normal RUN completes Set-Clipboard before its reply. A child that
outlives an agent restart can still land late; that case remains uncovered.

The original fake-clipboard regression run had two failures and one pass.
After repair, all 48 clipboard contracts passed, including exact-8-MiB,
one-byte-over, recopy, ownership and FIFO cases. Production example clippy
with warnings denied and structural budgets passed. No Mac pasteboard access.

Exact final project, hosted and required PR gates remain pending. These tests
prove the invalid-response state correction, not universal paste privacy or
live Windows behavior. Product state and criteria stay unchanged.
