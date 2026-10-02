# A19 three interruption cases — 2026-10-02

Evidence level: deterministic contracts and static review. This continuation
adds no live guest sample, release evidence or criterion promotion. Product
truth remains in `capabilities/windows-hvf.json`; A19 stays OPEN.

## Plan and boundaries

The T22 production lifecycle previously ran only
`staged-disk-verify-read`. The two additional stop points and schema-2 receipt
validators existed, but the lifecycle and collector did not produce their
observations. Connect both cases without changing the first restore or its
four guest boots and natural shutdowns:

- `swap-staged-disk-verify-read`: use an owned disk/vars clone pair, establish
  a selected generation distinct from the original snapshot, interrupt before
  publication, independently verify unchanged selected bytes, then retry and
  verify both restored hashes against the original snapshot.
- `create-staged-disk-hash-read`: use another owned clone pair and a fresh
  destination, interrupt before its manifest exists, verify continued absence,
  then retry and authenticate the manifest and both independently hashed files.
- Collect exact stop observations, retained FD logs, helper results and
  independently computed hashes transactionally. Require all three cases in
  new production receipts while keeping historical schema-1 verification.

The helper-only cases do not add guest boots or independent runs.
`run_count` and `sample_count` remain one on success; a completed production
receipt counts three interruption cases. Promotion flags remain false.
Physical power loss remains outside A19's stated criterion.

## Process and evidence ownership

Each added case owns its cloned disk and vars under the isolated live library
and retains separate proof artifacts outside that media tree.
The private FD log and observation context contain paths and never enter git
or public receipts. Stop observations bind the actual owned helper PID and
staged read; result JSON alone cannot establish byte identity.

Commands have finite deadlines and bounded cleanup. A lifecycle timeout whose
job-control descendants cannot be proven stopped preserves private media,
records cleanup failure and fences subsequent queue work rather than reporting
successful cleanup.

## Retained failed control-flow probe

A deterministic mocked probe of the immutable `e0f7ddc0` observer confirmed
that terminal `waitpid` status could be consumed during the staged-stop race,
yet the cleanup path still sent SIGKILL to that PID and left the subprocess
return code unset. The trace was SIGSTOP followed by SIGKILL after terminal
status zero had already been reaped. No actual PID was signalled, and this is
not a live helper or guest failure. Failed probe log SHA-256:
`d21808951c4fb394cf1fdd62e12f6fb64bc5a4df74f34bfa13dceb111ef3dac0`.
The probe's source SHA-256 is
`83818d7dba92d1c56ea41d731305f6783538aa868d655f97b0d1d221b3c6dac8`.
The failed trace remains failed evidence after the ownership guard is repaired.

A separate new command-group ownership regression initially failed: after
the session leader had already been reaped, cleanup emitted three additional
signals. Failed log SHA-256:
`fb75e5c44a1759a956e8bdb1fcecdcab1530f39888c6df6ac9cc0f1e34a99911`.
The correction probes a completed leader's group without signals, refuses
unproven residual cleanup, and reserves an unreaped leader's identity through
timeout signalling before reaping it. This failed deterministic trace is
separate from a live process-group failure and remains in the record.

The combined fixture run then failed one test and errored in another during
macOS group teardown (`killpg(..., 0)` returned EPERM). Failed log SHA-256:
`e2e2c1d455714f48b96bee9da95d63d31ac4f736bb924a84525260fb4592509d`.
Independent bounded owned-process probes found an unreaped, same-owner zombie
leader with EPERM; after the owned child was actually reaped, the group probe
returned ESRCH. Independent probe log SHA-256:
`e0f1955eddbc48ae00f28934db9b8983aa9bcde011b239e087d34db7dd78b828`.
This establishes those fixture states only. Generic EPERM is never accepted
as proof of absence: teardown must reap its owned leader and subsequently
observe an absent group, or retain media and refuse verified cleanup.
The [pinned Apple XNU signal code](https://github.com/apple-oss-distributions/xnu/blob/f6217f891ac0bb64f3d375211650a4c1ff8ca1ea/bsd/kern/kern_sig.c#L1581)
filters zombie targets before its EPERM decision; that source revision is not
claimed to be the installed kernel. The owned fixtures supply the local test
evidence.

New production-coverage assertions against `e0f7ddc0` failed twice because
the initializer emitted schema 1 and production admitted a first-only case.
Historical schema-1 receipts remain valid; the new producer's three-case
requirement is separate. Failed log SHA-256:
`fbe28a57be643d1aad3a7b5564267de58aa63dca76a4df57adecf81f00633189`.

Two final ownership checks exposed interruption during process construction
or waiting before cleanup was protected. A mocked auxiliary baseline marked
cleanup verified and deleted disposable work after constructor ownership was
lost (log SHA-256
`a72d85a9b75dcab6f314b12bba0994bdc1976989fa1944ff3e25d543107c808d`).
A regression with an actual disposable child also failed before correction;
only the test retained and reaped its original child handle (failed log
SHA-256
`591e7448c02ac16b64ab97b33c691b58c356e1f544ebea60c91f4c988b439258`).
The lifecycle counterpart initially failed three constructor/wait interruption
subcases (log SHA-256
`f4ae75ef11ea29092bfe8fdfb56d8b4cbb33991e164c25558df13b1f5bac7b9f`).
Both protected spawn paths now retain media and refuse verified cleanup when
ownership is unknown; they never guess an unreturned child PID. These are
failed deterministic fixtures, not Windows observations.

## Verification checkpoint

The new orchestration contract passed 14 tests, collection passed 13, and
process cleanup passed four. Existing observer, first-case,
publication and hash contracts passed with historical receipt compatibility.
All 31 new tests are deterministic fixtures, including actual owned process
interruptions; they execute no guest workload. The combined A19 contract gate passed 91 tests at source
`5621ebbb42a0b20d6bd2be4b01c6efb72b71c045`, with log SHA-256
`1fc67973f20e5f06516eba7591f5eff6926fdc90a5f8430d94bccdf2d630e5ea`.
The full exact-source checks and their receipts are recorded in the change's PR after this source checkpoint.
The earlier T22 single-run and accepted T23 campaign remain separate evidence
at their original sources and counts.
