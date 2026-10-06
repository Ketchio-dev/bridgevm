# Native VMM development fixture preparation

`d11-native-fixture-preparation` creates a fresh, private Windows development
fixture through the existing scripted installer. It is not a release criterion,
a product install journey, or a T15 sample. Its public receipt always reports
`DEVELOPMENT_ONLY`, `pass=false`, `claim_eligible=false`, `criterion_pass=false`
and `t15_ready=false`.

The stages are distinct: `installed` requires the scripted install marker and
natural shutdown; `ready_stopped` additionally requires a real guest READY,
running agent service, nonce-bound new-account/desktop observation and natural
shutdown; `sealed_fixture` additionally requires absent owned writers, normal
mount cleanup, immutable media and stable full hashes. Tests with mocked
providers prove orchestration contracts only. Installation fit and guest
behavior remain unproven until an actual physical queue job completes.

Build `scripts/development/build-d11-fixture-helper.sh ABSOLUTE_NEW_DIRECTORY`
from clean, exact source before admission. The small development executable
reuses the existing CSPRNG answer-file generator and production BootSeed/Secure
Boot routines. Its helper tree includes the exact source commit and bundled
resources. The product answer-file template remains unchanged. Generated
credentials occur only in required private answer files/media, never command
arguments, shared readiness scripts, public receipts or CI artifacts.

The private TSV input manifest has exact path/hash rows for `iso`,
`payload_manifest`, `binary`, `renderer`, `firmware`, and bounded tree-hash rows
for `payload`, `tools`, `fixture_helper`. Tree hashes use the canonical sorted
records implemented by `TreeSeal`; links are refused. The tools directory
contains `wimlib-imagex`, `bridgevm-catalog-verify`, and `bv-file-compare.exe`.
Metadata rows are `schema=bridgevm.d11-fixture-input.v1`,
`classification=DEVELOPMENT_ONLY`, `source_commit`, `binary_source_commit`,
`binary_profile=release`, `binary_features=venus`, `rust_toolchain` and
`container_gib`. TSV uses tabs, not equals signs. The helper source must match
the harness commit; the recorded runner source may differ. Review the actual
build artifact receipts before submission; these fields are not attestations.

The owned APFS sparse container has a fixed 16–32 GiB capacity. Admission keeps
104 GiB host free space plus 2 GiB overhead beyond that entire capacity. Its
installer is 16 GiB logical, target 64 GiB logical, and vars 64 MiB each.
Temporary media, share and bulk logs stay inside the cap. A logical sparse size
is not a fit guarantee. ENOSPC, timeout or cancellation retains an incomplete
fixture and its records, with no automatic retry. Cleanup signals only owned
groups, independently reconciles nested mounts, never force-detaches, and fences
the worker when cleanup cannot be proved.

Before the first D11 submission, upgrade and independently review the actual
installed worker's development cleanup dispatch and D11 guard. A new job
harness alone does not update an older worker. Run deterministic checks locally
for feedback and on hosted CI for the exact pushed source before live admission.
No ordinary deterministic test belongs on the physical queue.

The sealed backing remains on a distinct filesystem. It cannot directly serve
the existing same-volume T15 `cp -c` path. Export and home restaging require a
separate admission: full stable pair hashes, an exclusive destination, fresh
104 GiB reserve, and the reviewed strict sparse copy path. If necessary, first
export to owned external staging, verify, normally detach and retire only the
owned backing, then stage to home APFS. Preserve the small preparation records.
Archived receipts report retained historical proof. Current cleanup guards
check backing metadata identity; neither substitutes for downstream full
content hashing. Canonical media and other jobs' assets remain immutable.
