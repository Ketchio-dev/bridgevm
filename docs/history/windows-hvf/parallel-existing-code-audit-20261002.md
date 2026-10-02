# Parallel existing-code audit — 2026-10-02

Classification: deterministic failure reproduction and focused corrections.
This follow-up audited existing code beyond the earlier snapshot work. All
reproductions used synthetic data, test sessions or signed fixture executables;
no live Windows gate, private guest assets or product promotion is recorded.

## Host guest-evidence authentication

T19 stamped a complete lane with guest JSON containing only `nonce` and
`status: fixture`, provided its SHA-256 matched the result. It never checked
the raw journey observations, agent result, logs or host audio counters.
The existing T17 verifier rejected that same minimal guest proof. Separately,
the preceding guest verifier accepted Boolean lane and audio-error fields
when identical managed/shared agent files and their hashes were supplied.

T19 now invokes the shared nonce-bound guest verifier before writing a stamp.
Guest and agent identities require exact types, including integer audio error
counts. Nine regressions passed: complete proof succeeds; correctly hashed
minimal JSON, wrong nonce, type coercion, corrupted raw observations, missing
agent results, invalid shutdown evidence and failed host audio are refused.
The signed T19 fixture smoke passed its pilot and eight refusal jobs. Each
refusal produced no authentication stamp and verified cleanup. The existing T17
signed fixture smoke retained all 49 checks. These fixture outcomes do not
establish guest workload success.

## Signed entitlement verification

A genuinely signed fixture executable with `hypervisor=false` and an unrelated
later Boolean `true` passed the old text-matching entitlement gate. Shared
plist parsing now requires the requested entitlement to be an exact Boolean
true and checks release debug entitlement policy. Wrong types, duplicate keys,
malformed or oversized data and extraction errors fail closed. The packaged
HVF verifier and all three signing helpers use that parser. Five test methods
covering 18 signed configurations passed; this is signing-policy verification,
not proof that a guest can run.

## Focus observer lifecycle

Replacing a session on an attached framebuffer stopped its window-focus
observer without rearming it. A synthetic resignation test delivered zero
ordered KEYINPUT cancellations where one was required. A second failing test
showed queued resignation callbacks still firing after monitor retirement.
Session replacement now rebinds the observer, and a generation fence suppresses
retired callbacks. Three new regressions and nine related binding, ordered
input and pointer tests passed: 12 tests, zero failures. Synthetic AppKit
windows do not establish live guest behavior or UI responsiveness.

The display-click action also posted after an exhausted timeout: a 0.01-second
budget with a 0.1-second synthetic readiness read returned success and one
stub post after 0.101 seconds. Cooperative waiting now checks the deadline
before reading and again before posting. Four new fake-clock cases and four
existing display-click tests passed; the combined UI run passed 20 tests.
This bounds action admission, not the duration of an Accessibility operation.

## RND32 entropy packing

The 32-bit TRNG call used an eight-byte stride when packing its three response
registers. A 96-bit request therefore discarded entropy bytes and returned
32 zero bits while reporting success. Packing now follows each register's
width. [Arm DEN 0098, table 9 and section 2.4.2](https://documentation-service.arm.com/static/61dff1202183326f21772c2d)
specify contiguous entropy across W3, W2 and W1 for the 32-bit interface,
with unused bits zeroed. Sixteen protocol tests passed, including every request
length from 1 through 96 bits and provider failure returning no register data.
These injected-source tests verify packing and failure behavior; they do not
measure live entropy quality. The host CSPRNG remains the entropy source.

## Media persistence destinations

A synthetic vars persistence policy named the disk as its snapshot output.
The previous runtime accepted it and overwrote the disk with eight vars bytes.
Output admission now compares each slot's destinations with the other slots'
original inputs, selected inputs and snapshot outputs, and revalidates before
persistence. Resolved names, existing inode aliases, missing output tails and
macOS ASCII case aliases are checked without changing legitimate same-slot
policy. Twenty-four focused regressions passed, including late alias changes
and managed original paths; strict Clippy passed. This is deterministic
protection at the checked boundaries, not proof against every filesystem race.
Unicode aliases of absent output names remain outside the added ASCII handling.

## Verification boundary

Focused passing checks support the corrections above. Earlier failed
experiments remain in this record and prior history. Hosted full-check run `37027904395` for preceding commit
`570c3634f1641a35247ae145466d9896651828e1` completed successfully at
16:02 UTC; it does not cover these later corrections. Its local full check exited zero, but source changes
occurred during that run; it is not clean exact-head evidence. The complete
broad-audit local and hosted checks had not yet started. No earlier full
result is substituted for the complete latest source.
Hosted drift 37031757241 refused stale TRNG count 14 versus actual 16; corrected.
Final results belong in the PR checkpoint. No ceiling or threshold was raised; the
capability registry's open criteria and product state remain unchanged.

## Bounded log tail follow-up

Actual `TailOffsetReader` retained a newline-free synthetic log across sixteen
polls, growing from 1 MiB to 16 MiB. The extracted accumulator caps a logical
line at 2 MiB and discards an oversized line through its newline, including a
protocol-looking suffix. Ordinary partial UTF-8 and CRLF remain intact; file
truncation resets both partial bytes and discard state. Twelve focused tests
passed, including the existing tail cases. This is a bounded-memory/parser
result, not evidence of live UI responsiveness.

## Checked device queue addresses

Three local malformed-configuration tests reproduced panics in network,
console and GPU queue handlers when a ready queue used an overflowing driver
base. The original run failed all three cases; its failure is retained. Shared
checked offsets now refuse unavailable queue index/descriptor ranges and
overflowing used-ring writes before arithmetic can panic. Six new regressions
cover the three device cases, descriptor refusal without invoking its decoder,
used-ring overflow with zero writes and a normal used-ring update. The existing
queue-size clamp test was extracted unchanged. All 216 focused virtio tests
and strict Clippy passed. These synthetic device tests do not establish live
guest behavior or complete coverage of every device's address calculations.
