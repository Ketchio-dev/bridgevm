# Import hosted portability and attribution repair — 2026-10-03

Evidence rank: deterministic local and GitHub-hosted checks. No guest or
physical-Mac job was run. Product truth remains in
[`capabilities/windows-hvf.json`](../../../capabilities/windows-hvf.json).
The initial integrated source was `5e6131813d8cb14100185f1f031cf0b02cfa804a`.

## Failed exact-source checks retained

The full local project check finished with exit 1: 43 of 44 steps passed,
and documentation references failed. The frozen checkout remained clean.
Its complete log SHA-256 is
`afb9a70a9978ec475c83cd52a663a3ed7913777881fe4aa3f3daf91641dd90a6`.
The passing steps do not turn that overall result into a project PASS.

Hosted CI run 37094778966, job 111122362188, rejected five source identities
in the two new histories: two standalone development conclusion commits and
three retained intermediate Git tree objects. The objects existed locally,
but the fetched hosted history could not resolve them as repository commits.
The histories now label the retained development source SHA and distinguish
Git tree objects from integrated public commits. IDs and failed experiments
remain visible; the documentation checker and its criteria are unchanged.
The failed raw job log SHA-256 is
`29b091fcdab57666dfad6143a8c7b8710b4b1fac23d1966a3dac9f967fa9980d`.

The mandatory Ubuntu 24.04 import job used actual QEMU 8.2.2 tools, but daemon
linking failed on an undefined `getpeereid` symbol before any tests ran.
PR run 37094778870 / job 111122361796 and push run 37094762406 / job
111122313989 failed. The PR raw job log SHA-256 is
`d64e1b1a8f565f22fd75ce9647f243a8c45103137b1dfb5018826adf8f8cd1fd`.
This is zero executed tests, not a QEMU 8 contract result. A successful Linux
cross-check did not establish a linkable executable or working socket API.

The same-source macOS push job 111122313943 ran real QEMU 11.1.1 and passed
all nine methods: five CLI/socket chains, two raw-only and two release-helper
contracts. That useful result does not override the required Linux failure.

## Repair and required verification

The Linux repair uses kernel `SO_PEERCRED` credentials rather than declaring
the macOS `getpeereid` interface on Linux. Darwin behavior and UID authorization
remain in scope; failed or malformed credential queries must refuse access.
Hosted native credential tests and all nine actual import contracts remain
required on the existing QEMU 8/11 matrix. No platform, method or tool is skipped
to obtain a green result.

Documentation attribution was corrected without increasing either history's
structural ceiling. Local reference checking then passed. The source repair,
all documentation and actual-size budgets must precede a new capability-only
seal and another complete local/exact-SHA hosted project run. The initial
5e613181 failure remains a failure. No live result, threshold relaxation or
criterion promotion follows from this repair.
