# T22 trusted-shell admission and required-check failure — 2026-10-03

This records deterministic failures and a source repair, not a live A19 pass.
The [development-only queue history](d10-owned-pair-queue-admission-20261003.md)
and [failed native pilot](t17-native-chooser-timed-pilot-failure-20261003.md)
remain separate. No criterion, threshold or product state is promoted.

## Preserved exact-source failure

Source `91665c87ce70a14c9f7a62b06114064ae74ec89e` passed the local full project
check: 44 outer PASS, zero FAIL, 891.849424 seconds, exit zero. All 4,131 tracked
source records matched before and after. Its required hosted T20 checks failed:

- [PR run 37146355565/job111271006322](https://github.com/Ketchio-dev/bridgevm/actions/runs/37146355565/job/111271006322).
- [Push run 37146353070/job111270999037](https://github.com/Ketchio-dev/bridgevm/actions/runs/37146353070/job/111270999037).

Both recorded five source-boundary tests with one failure. The controlled-
environment test expected zero, but its `/bin/bash` launch returned -6 and dyld
reported that the inserted library `/never` could not be loaded. The contract
step exited one. That nonexistent path is owned test input, not a guest asset.
PR raw SHA `426c68de96dc62c7c9a89ea98a045067099d4f5afd37fa72e308579dd1887ba3`;
push raw SHA `d6790dbba219f5fdc42ecacd95d8c8a6c3f9f0fb67c4ea667b8b7a9d95eeee10`.
The local pass does not override either actual hosted failure.

The 19:34:26–19:34:55 UTC snapshot contains 105 applicable runs/146 jobs:
102 runs succeeded, two failed, one remained in progress; jobs were 142 success,
two failure, one source-conditioned advisory skip and one in progress.
At 19:35:19 UTC, manual full run37146418151/job111271195634 was still running
its exact project check, started 19:12:24 UTC. Standard PR37146355722 had thirteen
required successes plus its advisory skip. This is a dated checkpoint, not
an overall green claim or a result for the pending full check.

## Actual Windows shell scope

T22 PR37146355519 and push37146353083 jobs succeeded under PowerShell
5.1.26100.33438 and 7.6.6. Each actual raw contains the Windows fixture collector
PASS (`fixed=4 ntfs=2`), 72 unique case PASS records and the terminal mocked-CIM
72-case PASS. The same cases repeat in four contexts; they are not 288 distinct
cases. Numeric exit footers were absent; job/step SUCCESS is what was observed.
No production Windows provider query or prepared live disk/vars pair was run.

## Narrow source and fixture correction

A shell script cannot sanitize variables before its own dynamic loader starts.
An owned empty C entry control returned zero with a clean environment, -6 with
the invalid inserted-library value before main, and zero after removing it.
That demonstrates the boundary; it does not reproduce the hosted `/bin/bash`
environment, establish a SIP cause or prove guest behavior.

Source repair `0f08cadc1a6382f447330490d998df86facea8d9` clears loader overrides
with builtins inside an already-running trusted admission shell, before its
first child. The unchanged checked-Git function moves to a script-relative
module: fixed system Git/Perl, 30-second alarm, clean environment, exact HEAD,
dirty-source and cache refusals remain. No PATH helper override is introduced.
Existing ceilings stay unchanged; the two new modules are registered at 11 lines.

The fixture starts trusted privileged Bash, then exports the same two foreign
loader values as positional data before sourcing admission. Foreign Git and
shell-startup fields remain in its initial environment. All five original test
methods and nine assertion ASTs remain exact. The success body additionally
requires both loader variables to be absent. No skipped test, accepted signal or
weaker successful return substitutes for zero.

Guard/dispatch children before admission can still refuse on their own loader;
a foreign-environment cleanup fence does not isolate dirty-source detection.
The separate clean-environment dirty-code tests retain that proof. Initial
interpreter protection and all-entrypoint startup control are not advertised.

Focused corrected contracts: five tests PASS, zero failures; shell, budget and
diff checks exit zero. Complete successor local and exact hosted checks remain
pending. Existing native/guest criteria remain open, and no artifact build,
worker update, live preparation, release or promotion used the failed source.
