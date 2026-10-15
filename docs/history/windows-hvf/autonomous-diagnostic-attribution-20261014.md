# Diagnostic failure attribution — 2026-10-14

This is development evidence, not a release or capability promotion. It follows
the [media and fixture lifetime record](autonomous-media-and-fixture-lifetimes-20261014.md).
The exact successor validation result will be recorded after its source is frozen.

## Retained physical UI failure

The second overview diagnostic on candidate
`76c41c398bbac3497cf028213b2e6ebfe47a1f13` ran from 23:09:30 to 23:09:40 UTC.
The driver reported Accessibility trust after the operator-authorized permission
renewal. Its first `welcomeControls` request failed with raw AX error `-25201`.
It completed one request, zero mutations and zero scenario samples. All seven
scenario actions remained false. Both owned app exits and cleanup were observed;
individual numeric exit codes were not retained. Four partial appearance captures
do not establish a successful action scenario.

The failed receipt is retained with SHA-256
`945270afa7dd6a3c3221f8988dc7343d2fa23a56e76b294a53a71b5508ed1661`.
Its audit has SHA-256
`c9874bd170522dab4edd605659a362b73edf82d84b335d0b3ce98a61ed1a2be1`.
The earlier untrusted diagnostic and quarantined AMFI failure remain unchanged.
The latter still records cleanup false and an unknown child exit status.

The first hypothesis about the requested AX array maximum was wrong. Apple's
[API documentation](https://developer.apple.com/documentation/applicationservices/1462060-axuielementcopyattributevalues)
allows a maximum larger than the available array, returning through its end.
The original failed reply does not identify the failing API or attribute. Empty
arrays, unsupported shapes and preparation calls remain hypotheses.

## Bounded AX error context

Integrated commit `5d84be6e18d6d76920f38e699d9ad5c7986076d7` adds optional typed
API, attribute and element-scope context to failed private replies. Both the
overview and installation-cancel service retain the same raw error and terminal
refusal. The context contains only fixed vocabulary, with an `other` token for
unrecognized attributes; it contains no UI values or arbitrary attribute text.

The adapter still requests index zero and maximum 10,001, refusing returned
arrays above 10,000 elements. Ownership checks, message timeout, deadline,
cancellation, ambiguity handling, fixed mutations and no-retry behavior remain.
Legacy replies without context remain readable. Old strict readers reject the
new context-bearing failed replies, so host and driver must come from the same
candidate. No mixed-version compatibility is claimed.

Focused checks passed 454 Swift assertions and 55 Python tests. These include
108 checks through the production wrapper with injected system calls and 24
checks of the actual services' error mapping. The driver compiled but was not
launched. The same final JSON context test compiled against the exact old
protocol sources and failed canonical decoding; it passes with the new format.
That result establishes diagnostic-format support, not the physical AX cause.

The authored report has SHA-256
`b7777729080eba52da2c73b6b525b94ac628a5658dda46a85a098dd9ef79cc37`.
The independent committed-source review has SHA-256
`a92d7cb9c1d0db94b243b1aff82c0bba24b4d685fb74d0fd955c78b98b062c7c`.
The reviewed source, bounds and failed observations are preserved. A new sealed
physical run is needed to identify the API that actually fails.

## Child-fixture readiness diagnostics

Integrated commit `25c9bf608146b97221c9709cf95eb8b49ca0c561` preserves diagnostic
context when the owned subprocess fixture misses its readiness marker. The
original two-second deadline, five-millisecond poll cadence, escalation checks
and production ownership, signal and cleanup code are unchanged.

Among existing fixtures, only the ignored-TERM fixture now redirects stderr
into an exclusively created, private regular file. On readiness failure, the existing owner performs one
nonblocking status observation. The assertion records exit code or signal,
still-running or wait error, marker/root metadata and a bounded stderr prefix.
Stderr reads refuse symlinks and nonregular files, use nonblocking open and retain
a prefix of at most 4 KiB of input bytes plus a truncation indicator. Observation errors
become diagnostic context. Successful readiness does not sample child status or
read stderr. Fixture cleanup still removes the owned files after the assertion
has placed the context in the test log.

Focused execution passed 13 tests, with zero failures and one intentionally
ignored subprocess entrypoint that the outer ownership test invokes. Five new
controls cover an exited child with stderr, a still-running child, success-path
callback exclusion, capped/error output and a nonblocking FIFO refusal. Formatting,
structural budgets and staged whitespace checks passed. No full-project or
hosted success for this successor is implied by that focused result.

The frozen diagnostic receipt has SHA-256
`d3bba275b6abe051905929d3c66a8c1c831c333d86f2c8b98dc3e5294fea6456`;
the independent review has SHA-256
`305a50ebf130ae87d40e7393e3ed802868df10fd403ed877fdaec1912d69300c`.
The original readiness failure, binary and incomplete observations are retained.
This change does not identify or repair its unknown cause.

## Earlier checks remain distinct

Candidate `76c41c398bbac3497cf028213b2e6ebfe47a1f13` remains failed overall:
its local full check recorded 44 PASS / 1 FAIL at child-fixture readiness, with
the original cause unknown. Its 103 hosted workflows later completed with
102 successes and one failure, without cancellations. The failed manual CI
retains the T12 temporary-Git teardown error. Its separate hosted full check
passed all 44 executed stages; the unavailable optional Linux target was
explicitly skipped there and is not counted as an executed stage.

The sixth candidate `df70acf596237a0ed99e15e83ce197d4df00a96b` was published
separately. Its local full check finished at 23:47:18 UTC with all 45 stages
passing and all 5,101 tracked source files unchanged. Exact hosted checks were
still pending at that checkpoint. The local result has SHA-256
`e750550b33af630266a2dd57f43a583439f2c9729a174dde3b1c0ca509f47ba7`;
its complete log has SHA-256
`84086613d7f5d06bd4fca92249f3f4b0ad4656162c52fc71a35753d5b99366f0`.
This does not replace the required hosted result or prove the seventh successor.

Its prepared T12 executable, signature and eleven-row input manifest
were independently verified, but no runner or firmware was executed. The fixed
20/20 criterion and exact-head CI requirements remain unchanged. T12 preparation
does not prove Windows behavior, security, MSI-X or release readiness.

Product wording, criterion states, thresholds and known defects remain governed
by `capabilities/windows-hvf.json`. No result in this record changes them.
