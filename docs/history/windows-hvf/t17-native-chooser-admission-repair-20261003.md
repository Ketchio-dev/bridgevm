# Native chooser admission repair — 2026-10-03

Evidence rank: deterministic tests and source review. This checkpoint follows
the final focused checks and precedes integrated whole-project, exact-source
hosted CI and any successor live run. No criterion is closed by this record.

The preceding [timed pilot failure](t17-native-chooser-timed-pilot-failure-20261003.md)
remains failed. Its whole-call timings do not identify a slow AX operation or
a guest cause. The installer-share chooser runs before the runtime starts.

## Reproduced source gap

The actual old `acceptSelection` method was extracted without changing its
body and compiled with owned lookup, enabled-read and input callbacks. Each
read advanced a logical clock past the same deadline. The old method invoked
one input callback in each case, failing both no-late-input assertions.
The actual successor body refused both cases with zero input callbacks.
These callbacks perform no AX IPC, UI manipulation or guest operation.

Native IO now retains the checked clock and deadline. Admission surrounds
attribute names and values, relationships, snapshots and retry pauses,
activation, path writes, panel actions and selection. Waiting checks admission
before readiness, after readiness and after a pause. Once admitted, a matched
keyboard down/up pair completes once; expiration does not replay a pair.
Selectors and the existing total and activation timeout limits are preserved.

An already admitted synchronous AX call cannot be preempted. Existing failure
diagnostics also remain nonpreemptive. This repair provides cooperative
admission and returned-call checks, not a hard wall-clock bound.

## Preserved diagnostic failures

A late ordinary thrown read originally prevented input but omitted the fixed
outer operation's deadline refusal. An owned throwing-enabled-read witness
reproduced that diagnostic failure with zero inputs. The successor retains the
operation, returned boundary and bounded original error. Under-budget errors
retain their original object identity and text.

A later probe found 12 AX-code preservation failures across 301 original
prefix lengths after nested clipping. Selecting the first recorded code could
select a clipped partial value. The final helper retains the last already
recorded code; the identical sweep has zero failures and bounded diagnostics.
The earlier native 138-test and shim 336-test passes remain dated results,
preceding this additional failed probe; they are not the final conclusion.

The initial shim compilation failure is retained. Its unsupported assertion
helper was replaced with a supported assertion of the same numerical predicate.
Neither the diagnostic bound nor an acceptance criterion was relaxed.

## Final focused evidence

- Apple XCTest: 139 passed, zero failed or skipped; separate Swift Testing:
  23 passed. Raw SHA-256:
  `a5a8390b90bf3e705e62a908a8e5b3ca8cf53c969d335e52c6fca72579e01693`.
- Full product shim: 337 passed, zero failed or skipped. This is a shim,
  not Apple XCTest. Raw SHA-256:
  `38b48f1c2f0ecfb4869d494a66d7700dc5ac8a9b812a768ba67e7c19bf57fad2`.
- Root independently recompiled and ran all four final owned witnesses:
  old-body assertion failure, repaired-body success, late-thrown-read success
  and 301-position clipping success. Receipt SHA-256:
  `4c4311e8a054d4a57ca7e344f43bde2dcda0f3a9b304e969cb01b83db70b0fa6`.
- All 28 candidate paths and handed-off evidence hashes matched. Sixteen new
  budget rows use actual sizes; eight existing ceilings were lowered and none
  raised. Final staged diff and structural checks passed.

Focused and shim counts are separate observations, not an aggregate guest
success count. Whole-project and exact pushed-source hosted checks remain
required before using a successor artifact. No native chooser success,
guest success, release evidence or product-state promotion is asserted here.
