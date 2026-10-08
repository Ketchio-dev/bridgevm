# Install disk size admission — 2026-10-08

Evidence: native deterministic Swift tests; no Windows install or user-media run.

A JSON-roundtripped install request with `diskGiB=Int.max` terminated the XCTest
process with signal5 when the plan converted GiB to bytes by unchecked UInt64
multiplication. A second request representing2^63bytes avoided arithmetic
overflow but failed FileHandle truncation after staging had already been removed:
the prior4096-byte target became0bytes,8192-byte vars became the64MiB seed, and
the evidence log disappeared. These are distinct retained baseline failures.

The plan now returns an optional positive byte count representable by the signed
file offset. The creation factory refuses invalid sizes before reserving a bundle;
plan validation retains the64GiB minimum and reports unrepresentable values.
Staging checks representation before reading the seed or removing any old work.
No arbitrary product capacity cap, reservation or workload-fit guarantee is added.
Small synthetic staging sizes remain usable by low-level tests; product creation
still requires64GiB. Valid byte counts and normal preparation are unchanged.

Native focused tests42/0 cover negative/zero/Int.max, largest representable whole
GiB and its successor, JSON request roundtrip, no bundle creation, no staging
creation and byte-exact preservation of old staging on refusal. Existing install
and staging suites pass. The first staging test invocation failed compilation
because its MainActor annotation was absent; fixed before the actual failing
regression. System/developer UniversalHID duplicate-class warnings remain.

Exact local full and hosted checks remain required. No claim of live install
reliability, host-filesystem maximum size or criterion promotion. A11 remains OPEN.

Restoring unchecked conversion reproduces signal5; mutation removed, then all
128 Windows-install tests pass. Static review found no scoped defect; newly
registered files measured at actual size and existing reduced budgets ratcheted.
