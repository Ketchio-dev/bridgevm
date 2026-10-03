# Bound WinPE diagnostic evidence — 2026-10-03

Classification: deterministic tests and static source review. No live job,
Windows boot, companion installation or criterion pass is established here.
Product state and criterion policy remain in `capabilities/windows-hvf.json`.

## Reproduced boundaries

A tiny synthetic memory reader executed the retained development source SHA
`b991aeb421c439871e631839b37a4f2b62e5d6a3`. A 67,117,056-byte capture exceeded
64 MiB but allocated and reached that reader. The desired refusal assertion
failed with exit 101; no guest memory or Hypervisor.framework was involved.
Raw log SHA-256:
`37ee7168ca36db9910a180f851a277421bada5ee396358a2c3d55e235be1f771`.

Owned emitters also wrote 1,025 bytes through each old native/wrapper logger
under a 128-byte fixture bound, with normal exit 0 and no quota refusal. The
expected bound assertion failed. Raw log SHA-256:
`be6d37c27d03977dd6260cf648d2728254dcc543581a0b76414000fb06cde5b8`.

Intermediate drafts deleted a different-inode replacement at FIFO cleanup,
and reopening its pathname for native stdout truncated an owned sentinel.
These are controlled path-replacement fixtures, not an observed host attack.
The corrected writer baseline remains failed, SHA-256:
`943a199b7fa4aa776311e1a7f602e18cc040e90a12d1674048272065b35abca6`.

## Resulting behavior

D4 enables a closed diagnostic envelope: 64 MiB raw captures, 48 MiB plus 64
bytes PPM, 512 MiB each for run.log and target-stat, 16 MiB each for wrapper and
cleanup logs. At most eight capture pairs bound listed payloads to
2,046,820,864 bytes. This bounds those producers, not total host storage.
Geometry and GPA addition are checked before allocation or guest-memory reads.
Oversize, incomplete, ambiguous or failed producer evidence is refused; prior
positive status cannot cover a later failure. Refusal retains diagnostic media.

The native probe keeps its direct PID. Logger writes use the acquired FIFO
handle; dev/inode/type checks preserve replaced paths. Host-CSPRNG readiness
nonces fail closed and new packaged Python calls use an absolute system path.
Both product and debug package helpers include the full extracted module set.
The opt-in envelope rejects agent, vTPM, disk harvest, GPU, boot-timer and input
producer combinations. Existing default behavior remains separately checked.

D4 keeps 300,000 ms watchdog, 360 s outer timeout, 4 GiB RAM, 4 CPUs, 50 million
exits, seven original checkpoints and final capture. Three-dimensional graphics
and disk harvest stay disabled. Its receipt remains explicitly nonpromoting.

## Focused source and retained corrections

Retained development source SHA
`66d33a85f4434940a323d175a7a4afd698b872f7` is integrated as `e5be79c9`.
The affected project step passed 20 suites/189 tests; its raw log SHA-256 is
`26d96e49f3d321a7be7e3c92774884a39f3e6ceef36cb616cf4efbfb79804f55`.
Native RAMFB 20 tests passed, including 9 new bounds/status cases. Sink 19,
D4 collection 51 and actual launcher 10 tests are included in the affected step.
Eight compatibility smokes, reader suites, example clippy and statics passed.
Independent reviewer replays and package/source comparisons found no blocker.
Root compared 25 changed source files byte-for-byte and retained all prior
project command paths; two new diagnostic contracts enter the aggregate gate.
Thirteen new budget rows use actual size; 11 existing ceilings decrease.
No existing ceiling increases. Full integrated/project/hosted proof is separate.

Fixture lockfile/environment/format errors, a zero-test filter, moved mocks,
missing completion proofs and wrong test expectations remain retained. A tiny
Darwin timeout returned conservative cleanup false; its fixture now requires
refusal and no live owned children, without weakening production cleanup.
One host-pause smoke exited0 despite an unbound-variable error in its raw log;
that compatibility defect was repaired and replayed. ShellCheck also caught an
extracted dynamic local; explicit arguments repair it.
The retained development source SHA
`6cbe7f8c3dc0bea900780ce527716b4571cbf3ef` preceded a budget-only amendment:
an inherited two-column 258/implicit 0 entry gained a duplicate row. It is now
lowered in place to 241/0, with the duplicate removed and source bytes unchanged.
No failed experiment was relabeled as success. A matching checked artifact is
required before any genuine D4 hardware job; none has been submitted here.
