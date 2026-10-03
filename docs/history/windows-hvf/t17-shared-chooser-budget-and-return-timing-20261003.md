# T17 shared chooser budget and return timing — 2026-10-03

These source changes close a reproduced admission reset gap and add bounded
failure timing. They do not establish the cause or repair of either failed
Windows pilot. The [8a4 failed pilot](t17-runtime-chooser-pilot-failure-20261003.md)
and the earlier `035a889b` failure remain failed.

## Reproduced admission gap

At source `8a4a390d8b5a3c1d6f787b9bbd287064b1583a8c`, target-button lookup
finishes before the file chooser starts its interaction deadline; opening has
another deadline. The owned native fixture extracts the actual controller body
and uses the original lookup, opening and chooser code with no-IPC targets.

With a 20 ms fixture budget and a delayed target lookup, the old route sent one
open input and returned selected=true after 48.256 ms. Compilation exited zero;
the regression assertion exited one. This is a deterministic fixture result,
not a Windows workload, real panel timing, or a reduced production criterion.
The failed baseline is preserved.

Integrated repair `de4dc359f20234ecd5a7b1eac26e736c4fca7b14` starts one
monotonic budget before target lookup, passes remaining lookup time and the
same deadline through opening and confirmation, and refuses returning late
lookup before any further input. Accepted opening AXPress that returns late
now fails at the open-control return boundary. Slow accept-selection still
prevents entry to selection confirmation. Production caller timeouts, including
the existing 20-second runtime chooser timeout, are unchanged.

## Bounded failure timing

Integrated source `f801066c0f6b99c32a8d655aebe828b94d4936fe` records twelve
fixed stage labels, entry/return elapsed time, remaining budget and last
completed stage. The buffer keeps at most sixteen records; the rendered timing
suffix is capped at 900 characters and retains the six most recent records
that fit. It contains no paths, AX values or additional UI observations.

A captured clock origin and sticky checked reads reject observed backwards or
nonfinite clocks before later input. A late AXPress keeps its known result in
the error; an unavailable outer result is labelled not-recorded.
Accept-selection timing includes panel/button/enabled reads and AXPress as one
call. It does not isolate AXPress. Failure-stage timing can also include the
existing diagnostic work on that error path. Synchronous calls are not
preempted; these are observed return boundaries, not a hard wall-time bound.

## Actual checks and preserved limits

The first repair passed 44 native Swift tests through the repository XCTest
shim, with zero failures or skips, and compiled the full ProductE2E module.
Its retained argv does not independently prove env.sh sourcing; that provenance
limit is preserved. The final changed candidate used a retained wrapper that
sources env.sh and records the selected toolchain without dumping environment.
It passed 56 shim tests with zero failures or skips, compiled the full module
and passed the registered structural budget check. These are not Apple XCTest.
The extracted actual controller fixture now refuses with zero open inputs;
its compile and run both exit zero and its failure includes timing context.

An initial test compile failure, a later escaping-closure compile failure,
and the draft clock-origin/inner-read findings remain in private receipts.
Final inventories match before and after the passing checks. Existing ceilings
were lowered or preserved; new modules are registered at actual size, and all
116 legacy two-column budget rows keep their format.

- Old regression raw SHA: `d218ad8534b7a6084fa10efbb0b15cc082e6948679912bccea93a12eb3a04aba`.
- Final 56-test raw SHA: `2091d7739ff5a7bbddcfdd6b35887e079d50888b5ae542c791524c9dd3ffabaf`.
- Final controller raw SHA: `52128195d024db7f24fb7aacba7e6f363ba188277285fac329579980b8c1bae1`.
- Final source handoff SHA: `3c42fabf013e792949e7995ff4208a9a1015957c59a9a78df86b93cdaca8f4ce`.

Complete project and exact-source hosted checks remain pending at this source
checkpoint. No new live artifact or run proves repair; A9 and A19 remain OPEN.
Product wording, receipt schemas, guest contract and fixed gate counts are unchanged.
