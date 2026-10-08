# Product install scratch and retained-source volume — 2026-10-08

The owned-result-volume E2E repair does not by itself route a genuine app's
WinPE source-builder scratch: LaunchServices does not provide the caller's
TMPDIR contract. The builder now creates/canonicalizes the output parent and
allocates unique ISO/destination mountpoints and provisioning scratch there.
Its payload-staging child receives that root as TMPDIR. The generated image
and scratch therefore follow the selected library/cache volume, with the
existing detach and cleanup behavior unchanged. Packaged apps must include
this script; editing a checkout alone does not alter an installed app.

Queued T17 previously retained T19 sources below HOME, independent of the
queue's device. T19 correctly refuses cross-device canonical sources before
APFS cloning. Moving retention into the running job itself is also wrong:
its absolute manifest paths would break when the worker moves running to done.

Queued dispatch now derives `<queue>/t19-sources/<job>` only from an exact
owned `<queue>/running/<job>` layout. The private canonical parent must share
the job device; symlinks, unsafe permissions and existing destinations refuse.
Retention remains outside work cleanup and state-directory movement. The
existing explicit direct-tier destination, no-overwrite publication, immutable
source locking and all five T19 device/clone checks remain unchanged. CLI and
installer share private queue-directory creation independent of caller umask;
state-leaf symlinks/non-directories/unowned directories refuse before chmod.
Existing root aliases require an already-private owned target; terminal syntax refuses.

Synthetic source-builder fixtures distinguish caller TMPDIR from an initially
absent output parent, check two unique mount roots and preserve attach/detach
and readonly ISO assertions. A synthetic dispatcher-to-tier handoff verifies
the retained manifest and unchanged hash after running-to-done movement;
T17 smoke50 passes. Destination malformed-layout/symlink/permission/device
contracts3 pass. Actual CLI mode/refusal contracts4, installer launch3/dry11
and policy103 pass. The mock preserves argument boundaries as JSON and
exercises spaces/apostrophes. These are not real Windows installation receipts.

Two failed fixture attempts remain in the lane logs: the first source test
accidentally changed mock readonly equality to2 while doubling invocation
counts; the first queued handoff name omitted the existing fixture's handoff
marker and therefore did not create its64MiB vars. Both fixture mistakes were
corrected; no production validation was weakened. A later installer fixture
initially attempted to unlink a not-yet-created bash stub; those3 errors remain
in installer-r1, corrected setup passes3. Restoring the old mountroot location
failed the layout assertion (mutation_exit1); production repair was restored.
The first raw trailing-slash symlink regression failed before repair, proving
the new helper could adopt/chmod its target; queue-storage-r3 passes all4.
Full7c83378f FAILED existing V2 queue-alias claim coverage; root-alias admission
was repaired, not the test. Full74e58ace passed. A later raw/resolved newline
transport regression failed then passed after rejection; new full/hosted pending. A9/A11/A19 OPEN.
