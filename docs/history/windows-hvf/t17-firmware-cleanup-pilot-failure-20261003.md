# T17 installed partial stage, failed product journey — 2026-10-03

## Frozen live result

**Evidence rank: live single failed run.** Job
`codex-t17-035a889b-firmware-cleanup-pilot-r1` used sealed source
`035a889b83df87243d6a52c001cd02acb7d4584d`, tree
`12c5b98ca3f2`, a development-signed app and 3D disabled.
The [original public receipt](../../windows-arm/evidence/t17-035a889b-firmware-cleanup-pilot-failure-20261003.json)
is preserved byte-for-byte: 2,652 bytes, SHA-256
`a9a196089296c1792fc9d32f6ed64be277e4eb8516272f2073304b28efb10d27`.
Receipt finish is 09:36:53 UTC; worker finish is 09:36:54 UTC.
One attempt produced zero passes and one failure: `outcome=failed`,
`failure_code=product-model-failed`, `worker_cleanup_verified=true`.
The exact-source strict verifier accepted both identical receipt copies;
its exit 0 validates an honest failed record, not a product pass.

The existing 463-byte authentication stamp validates the frozen lane result.
Artifact preflight, VM creation, source preparation, Windows installation and
Secure Boot provisioning each have one authenticated pass. All ten remaining
journey stage counts, beginning with first READY, are zero. Installation is a
proven partial stage; the complete product journey failed.
`pass`, `claim_eligible`, `criterion_pass` and capability promotion remain false.
The public receipt's final disk, vars, Secure Boot receipt and guest-evidence hashes remain absent.
No T19 handoff or live import sample is established by this run.

## Failure and retained observation limits

**Evidence rank: authenticated lane failure plus static source classification.**
The lane code is `input-selection-failed`. Its selection-confirmation deadline
entry guard expired before the final panel/path checks. This is a pre-first-READY
host-share chooser failure in the sealed ordering; it does not establish a guest
boot cause, a wrong selected path, per-action latency or why the deadline expired.

The genuine pre-submit observer ended with 21 samples at 30-second cadence.
The final sample refused foreground/instance continuity after a new app instance;
`complete_sampled_observation` and `continuous_state_proven` are false.
Its envelope remained 10,800 seconds and at most 361 samples. Sampled host state
never establishes continuous foreground/unlocked lifetime. Helper registration
metadata is not all-process absence.

A separate post-terminal observation overlapped app relaunch. Observation-triggered
relaunch is an inference only and is not the authenticated chooser failure's cause.
Root's owned-artifact cleanup recorded termination and one absence observation at
09:39:31 UTC. It does not retroactively make the refused observer complete.

Root's session observed the final installer log at 7,756,518 bytes before
deletion; no durable raw-log copy succeeded. The retention attempt found its
first source absent and copied zero files;
that failed capture stays preserved. Only the private 60-second progress frame
is retained. No raw guest log, framebuffer, media or secret is published here.
No guest failure cause is inferred from the missing diagnostic capture.

## Exact-source deterministic evidence

**Evidence rank: automated tests.** Exact 035a passed 44 local project steps.
[Hosted full run 37109530494](https://github.com/Ketchio-dev/bridgevm/actions/runs/37109530494)
passed 43 applicable driver steps; the absent Linux-target check stayed an explicit skip.
[Standard CI run 37109523865](https://github.com/Ketchio-dev/bridgevm/actions/runs/37109523865)
passed 13 required jobs, and companion runs passed 11 required jobs.
The current optional native CGL check still failed with error 10002/exit 101;
aggregate CI success is not a graphics pass. The producer receipt's false
hosted/security CI flags and absent run fields remain unchanged.

The [earlier 8e64 failure](t17-firmware-admission-and-install-cancellation-20261003.md)
remains unchanged: original 2,661-byte receipt SHA-256
`5cc2720e60cd82259d2b41172b89bd1193c901ce99053588c597743859387083`,
zero authenticated stages, zero passes and false cleanup/eligibility/criterion flags.
Its separate manual retirement did not rewrite that receipt. This new partial
installation and verified cleanup do not turn either failed journey into success.
A9/A11/A19 remain OPEN; no criterion, product state or capability wording is promoted.
