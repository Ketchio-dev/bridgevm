# T17 missing-report diagnostic privacy — 2026-10-08

Evidence: deterministic regression and source review, not a Windows/live run.

## Reproduced failure

The LaunchServices preflight's missing-report path read the helper stderr file
with `read_text()` and only then sliced the decoded text to512characters. The
slice did not bound the read, and the resulting refusal could disclose private
paths or other arbitrary helper diagnostics to the caller's stderr.

A synthetic actual-preflight invocation writes a private sentinel/path into
that error file and produces no report. The baseline refuses submission but
prints the sentinel. The regression failure is retained; no private media or
real LaunchServices invocation is involved in that reproduction.

## Repair and scope

The missing-report branch now emits only a fixed refusal and does not read the
error file. A helper failure still refuses before job submission. The report
reader, manifest/hash checks, trust requirement, launch arguments and deadlines
are unchanged. No raw helper diagnostic is promoted to a public error message.

The regression asserts both absence of the sentinel and that the stderr file
was never opened by `read_text`; it is included in the product contract smoke.
Existing admission, report-origin and bounded-reader contracts remain required.

This does not repair LaunchServices spawn failures, grant Accessibility, prove
helper termination after a failed launch, or cap the helper's on-disk output.
No private-key operation, permission grant, queue/fence change or live admission
is part of this change. Product state and A9/A11/A19 status remain unchanged.
