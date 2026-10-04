# T22 loader boundary — hosted full failure retained, 2026-10-03

This is the terminal follow-up to the dated
[required-check checkpoint](t22-loader-admission-required-check-failure-20261003.md).
Source `91665c87ce70a14c9f7a62b06114064ae74ec89e` remains a failed hosted candidate.
Its local 44-step success does not override required GitHub-hosted failures.

[Manual full run37146418151/job111271195634](https://github.com/Ketchio-dev/bridgevm/actions/runs/37146418151/job/111271195634)
completed as FAILURE at 19:36:41 UTC. The raw exact-source assertion matches
`91665c87ce70a14c9f7a62b06114064ae74ec89e`. It records 43 applicable outer headers:
42 PASS, one FAIL. The failed header is Windows product and A19 live-tier
contracts. Its five-test source-boundary suite fails the same source-identified Bash launch
assertion: -6 instead of zero, dyld reporting the nonexistent `/never` library.
Terminal project check reports one failed step; runner exit is one at
19:36:38.529702 UTC. Raw PID executable identity was not independently observed.

Raw size 2,199,127 bytes; SHA-256
`9e091097232fadfa9dd1493b3f53ece50f651af66888fe6d18740d967c6e5c03`.
Root independently rehashed and checked all 43 outer closures, the exact-source
marker, trace and actual exit footer. No rerun, cancellation or result retarget
changed this experiment. Linux target-absence handling is separate from those
43 headers; native conditional skips do not establish guest passes.

Final metadata, observed 19:37:26–19:37:56 UTC, is terminal: 105 runs/146 jobs.
Runs have 102 successes and three required failures. Jobs have 142 successes,
three required failures and one source-conditioned advisory skip. Exact-source
check runs match all 146 job IDs. PR303 rollup has 145 items and omits the explicit
manual full job; its smaller two-failure count does not make that job disappear.

The three failures are both T20 companions and the explicit full project check.
T22 Windows mocked-CIM successes remain separately valid within their stated
scope. No actual Windows provider query or new owned live pair was produced.

The narrow source correction and five original assertion-preserving tests are
recorded in the checkpoint history. Complete successor local and exact hosted
checks remain pending. A9, A11 and A19 stay OPEN; product wording, thresholds,
release flags and known defects remain unchanged. No artifact build, worker
switch, live retry or release used this failed source.
