# Windows install continuity: deterministic evidence, 2026-09-15

Classification: historical evidence record. The [capability registry](../../../capabilities/windows-hvf.json)
owns current status and wording; the [capability matrix](../../windows-arm/capability-matrix.md)
renders it. This record supports A11's install-continuity checkpoint and preserves
its failed full check. It changes no criterion and does not close A11.

## Current source and focused execution

Commit `16dc2bf927692d818da8a5907428bdef158626e1` retains Windows install sessions
in the library, accepts Start synchronously, and handles cancellation before
dispatch. Active sessions survive view reconstruction and library reload; inactive
sessions can be replaced when their request changes. Completion reload is wired
by the library accessor rather than depending on the view appearing.

The expanded native product-flow filter plus `LineAccumulator` passed **125 tests
with zero failures** locally in 30.01 seconds. It includes **4 admission tests and
8 session-store tests**. The receipt records base commit
`85c7ca6ecddf21b04f6ddc21c9d26e3044527d33`: the check ran before the later commit,
with the implementation present in the working tree. All nine recorded product
and test file SHA256 values match their committed contents at `16dc2bf9`;
those files and the native snapshot helper remained unchanged during the check.
This correspondence does not claim that the entire later commit was tested then.

The retained admission baseline executed one test with four failed assertions,
including two accepted starts where one was expected. Its queued work never ran.
The initial combined check failed to compile because a nested test fixture lacked
its own `@MainActor` annotation; the corrected fixture is included in the 125-test
pass. Neither earlier failure is reclassified as a success.

A separate text-comparison receipt reverses only the enumerated admission and
test-seam edits in the extracted session. Original and normalized session text
both hash to `225e08ed52bd392cb7e6804295ee0f9c4e22a9399b100e09677227c4ca4fe964`.
This supports the limited observation that existing install process/media
execution was unchanged. Independent source review found no introduced blocker;
source review and text equality do not establish live installer behavior.

## Failed full check and preceding checkpoint

The full `scripts/check-project.sh` run started at **2026-09-15 03:00:59 UTC**
with HEAD `16dc2bf9` and ended after **153.88 seconds**, exit code **1**, without
timing out or changing HEAD. Its final summary reports one failed step:
`capability evidence`. The diagnostic was `A11: its measured figures appear in no
cited evidence`. This document repairs the missing public evidence reference;
that recorded run remains **FAIL**. A new full local check and exact-current-SHA
hosted checks are still required at this observation point.

The preceding full local check passed in **180.87 seconds**, exit code **0**,
with HEAD `14fe6140d800812a03a1d7bc7de08135232c47e7`. Its seal receipt records
that the checked contents were committed without changes as
`dae0ef96bfd66951e452c1b4120d42fdf24ae0ab`. The full Git tree identifier
is retained in the hashed seal receipt below.
At **2026-09-15 02:58:28 UTC**, the retained hosted observation lists **78 runs**
for that exact sealed SHA, all completed successfully, including
[CI 34922274882](https://github.com/Ketchio-dev/bridgevm/actions/runs/34922274882)
and [Security and quality 34922274734](https://github.com/Ketchio-dev/bridgevm/actions/runs/34922274734).
Those results cover the preceding checkpoint, not `16dc2bf9`.

A second full check started at **2026-09-15 03:11:46 UTC** on the same HEAD
and failed after **150.78 seconds**, exit code **1**, solely at documentation
references. The checker interpreted this document's Git tree identifier as a
commit. The duplicated tree identifier was removed from this prose; it remains
in the original hashed seal receipt. The earlier focused reference check had
excluded this then-untracked document. The document is now staged for that
check. Both full-check failures remain retained; neither is a passing result.

## Retained receipt identities

Names below are relative to the operator's retained
`hvf-continuation-12h-20260914` evidence collection. Receipts and logs remain
outside the repository; this public record contains their identities and outcomes.
The local seal receipt refers to `project-check-first-run-ux.log`.

| Receipt | Receipt SHA256 | Log SHA256 |
| --- | --- | --- |
| `campaign/install-admission/hosted-filter-final.json` | `a2936bdcdfc1dc80d4df3d09ef32fd6d78a9892b99cb8344d8542fcc1b966759` | `d2f18608eca479d446176f211d90507e984c5164c0561aef7a238b3542c5799c` |
| `campaign/install-admission/before.json` | `8aad26b1206cdc28488b5b6f9aef067c14f333387469839fc64e75f813e7db0e` | `23084b5f063ae7bea11b4ebab052e2d5ba35d046cd6bc7c291e7bfc5c9dd780f` |
| `campaign/install-admission/initial-compile-failure.json` | `36a6e783a8b66800004b776de18a3fb21894dd06714389c251afefec0785a973` | `b341e3d67ef37914770222a539bb4a87219e0363330c4e5317f0888f3adc6149` |
| `project-check-install-continuity.json` | `acd20bb7ff87ab6c1162be5af30c78d82376d93a9929b2b8927f01f237a10053` | `12af4ae8af8dac02cbe9091557435f6b2e1587fb1fdebd86e3e088d876e99185` |
| `project-check-install-continuity-repaired.json` | `953f10999c4a88781240723be31c697ecb211d01592f3f57128adffcbb55fa7b` | `f5f640a6b1789beb0565e633ca25c4f205a3b14cad16c46755bcf3d31a2c1b9f` |
| `local-check-seal-first-run-ux.json` | `e52ebb435166d4cf56f36f6dba9c91dfbaadc0265f09f098b0def89f50c240c8` | `e6b435834046117a4b6232c0a021969b2450fb1e932acd750d7f9a5ea10bda05` |
| `first-run-ux-hosted-later.json` | `fac67cef73e87c79d2e332225b85d2f8f2d16f7d3c397caa527601b97e26b30e` | Hosted run links above |
| `campaign/install-admission/pipeline-extraction.json` | `90c938cbf1c8ba2965b6c9d126bd19dc8e0e9b1f0bc92b9ff0db6b0b0a24371b` | No execution log; text comparison only |

## Limits

The new tests use validation and scheduling seams, owned fixtures, and actual
view constructors. They do not execute the installer pipeline, launch a VM, or
render a window. These results do not prove live installation, visible window
continuity, mid-pipeline cancellation, external deletion/movement coordination,
or interruption recovery. A11 still requires its final release regression seal.
Earlier observations and failures remain in the
[A11 observation history](a11-observations-through-20260915.md) and repository history.
