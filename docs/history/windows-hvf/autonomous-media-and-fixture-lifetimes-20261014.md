# Media admission and disposable fixture lifetimes — 2026-10-14

Evidence rank: **automated tests and static review**, with prior failures retained.
This follows the [fifth checkpoint](autonomous-selection-and-diagnostic-launch-20261014.md).
This records four integrated conclusions at `a36f681d4c96b3e4108df78b5b4495bf98c89687`.
Combined full-project and exact hosted checks for the resulting candidate remain required.
No product state, criterion, known defect, evidence threshold or capability is promoted.

## Fourth and fifth failures remain separate

Exact `d913596b51df939442a19d60b689ea1ab61227e3` finished its local full check at
22:19:16 UTC with 43 passed and two failed stages out of 45. Both Rust selections failed
incomplete PCI boot-media fixtures; the later focused fixture repair does not rewrite that run.

Exact `76c41c398bbac3497cf028213b2e6ebfe47a1f13` finished at 22:55:55 UTC with
44 passed and one failed stage out of 45; all 5,096 tracked files remained unchanged.
The workspace stage failed `owned_child::tests::ignored_term_escalates_after_two_seconds_and_reaps`
at the fixture-readiness deadline, before the escalation assertion was reached. That crate
reported 60 passed, one failed and one ignored. Its failure also prevented that stage’s
portable-import substep from running. Escalation was not reached; the cause remains unknown.
The retained unchanged binary then passed the exact method on the original temporary volume
and on SSD, the owned-child group passed four with one ignored, and parallel/serial whole-crate
replays each passed 61 with one ignored. These bounded replays do not recreate the original
whole-project environment, establish a cause, identify a repair or replace the failed full run.
The four shim selections passed 425 / 1,408 / 65 / 379 methods, with three existing Control
skips. Those passing selections do not make the full candidate green.

A separate [hosted failure](https://github.com/Ketchio-dev/bridgevm/actions/runs/38649715356)
checked out exact fifth source and reported one teardown error across five T12 refusal tests:
`test_changed_binary_and_firmware` reached `TemporaryDirectory.cleanup`, which refused a
nonempty `.git` directory. The traceback does not report a sealed-input assertion failure.
The exact hosted writer is unidentified. This is distinct from the local owned-child failure.

## Snapshot restore requires a regular media file

Integrated [c43766f63778](https://github.com/Ketchio-dev/bridgevm/commit/c43766f63778b5395318f4b0a4f06c39c40de4dd)
changes the legacy `VmStore::restore_snapshot` disk and suspend admission from existence to
regular-file checks. Directories, directory aliases and FIFOs previously returned success
and changed selection/restore metadata. Regular files and aliases to regular files remain
accepted; no containment or image-format policy is added.
The exact final 12-method baseline passed six and failed six. The same tests passed all 12
after repair; the storage package passed 123 unit, eight existing integration and 12 new
integration methods. The focused 12 are that same integration subset, not extra unique tests.
The first experiment had four product failures and two socket-path setup failures; its
socket result is not promoted into a restore defect. FIFO successors retain six product failures.
Refusals preserve active-disk, runtime-state and prior last-restore bytes. Suspend refusal
still updates its pre-existing `image_exists` cache to false; complete metadata immutability
is not claimed. Existing error variants/path payloads remain; displayed text names non-files.
Clippy, formatting and structural checks passed. Tiny synthetic bytes establish admission
and metadata behavior, not usable disk/saved-state content, transactional restore or guest
correctness. A replacement after the metadata check remains outside this repair.

## Test cleanup ownership begins after successful creation

The first combined review blocked the storage fixture: its cleanup owner existed before
exclusive directory creation, so an `AlreadyExists` panic could remove a preexisting path.
A behavior-preserving extraction retained that original ordering in the shared constructor;
this baseline was not an untouched base binary. Its 14 tests passed 13 and failed one:
a caught creation refusal was followed by a missing synthetic marker. The failed binary and
log remain archived, alongside the initial formatting-only check failure.
Integrated [a36f681d4c96](https://github.com/Ketchio-dev/bridgevm/commit/a36f681d4c96b3e4108df78b5b4495bf98c89687)
constructs the cleanup owner only after successful exclusive creation. The same collision
control now preserves marker bytes and directory identity; a successful-creation control
still removes the owned child while preserving a sibling marker. All 14 tests passed,
and the storage package passed 123 unit + eight import + 14 restore/fixture methods, 145 total.
The original 12 media assertions are byte-identical. Only test support and budgets change;
production restore behavior is unchanged. Formatting, Clippy and structural checks passed.
Independent review cleared this issue; no post-creation hostile replacement guarantee is added.

## Source-build guidance separates app-side installation resources

Integrated [f4e1b31f75ac](https://github.com/Ketchio-dev/bridgevm/commit/f4e1b31f75ac10d34ff81484fdd7fe3f6fe8f3ee)
adds five prose lines to the install guide. The development-bundle command does not prepare
the complete app-side Windows ISO-install resources; an ISO and signed driver payload do not
supply them. The note links the existing full HVF app packager and required pinned wimlib builder,
without adding untested setup commands or changing the earlier toolchain guidance.
The real installed-boot resource copier produced 11 public files in an isolated fixture;
two mandatory installer helpers were absent there and in the fresh checkout. Static source
tracing distinguishes this from DEBUG checkout/local-helper fallback. No full app bundle,
native installation validator or actual ISO installation was executed for this finding.
Landing HTML/CSS/logo/archive assets remain unchanged. Actual 1,440 / 390 / 320px Chromium
views had no horizontal overflow; four served asset bodies matched source bytes. All three
limitations started open, and skip navigation, Enter/Space disclosure controls and the public
build CTA worked. Local console errors were zero. A separate GitHub navigation endpoint
returned 404 while the requested guide returned 200 with the expected build heading.
Of 39 actual Tab stops, 38 bounding boxes fit vertically; one exceeded the viewport bottom
by 0.015625 CSS px. The strict containment assertion remains failed, without an assertion
that every focus outline was fully contained. No meaningful layout repair was established.
Landing, documentation, references and budgets passed; own browser/server were closed.
No public deployment, physical-phone, fresh-machine build or Windows-install success is claimed.

## Disposable Git fixtures do not start automatic maintenance

Integrated [2f7516b2f763](https://github.com/Ketchio-dev/bridgevm/commit/2f7516b2f76318bcc576ae1f6afad9d780a8d478)
disables automatic Git maintenance only inside the owned disposable test repository before
its add/commit. Strict cleanup and the existing admission, refusal, runtime and worker
contracts remain unchanged. It does not ignore teardown errors or retry away the failure.
The original five refusal tests and immediate cleanup passed locally, while trace evidence
showed a detached maintenance writer overlapping deletion and exiting with errors. The new
lifetime regression failed against unchanged setup and passed after repair. All 16 T12
synthetic contracts and the same combined hosted-step command passed locally.
Local Git was 2.54.0; the failed hosted job used 2.55.0. The local writer trace supports this
bounded fixture-lifetime repair but does not identify the original hosted writer or prove
successor hosted success. No actual NVMe workload, T12 hardware sample or threshold changed.

## Correcting public-reference provenance

The earlier fourth 407-document PASS remains a real check, but its store was mixed-origin:
its own receipt records fetching candidate history from a local source. Describing that
store as pristine GitHub-only was wrong. Its old receipt and object store remain unchanged.
A new, initially empty store fetched only the recorded public GitHub heads and tags, with
no local fetch, alternates or borrowed objects. The unchanged checker independently passed
407 current documents at exact fourth source and 408 at exact fifth source. The extra one
is the new history document, not broader checker coverage. Six unpublished author objects
were absent while their six integrated commits were reachable. Availability is bound to that
fetch snapshot; this does not certify every semantic claim, the full project or hosted CI.

## Evidence bindings

This table omits private paths; retained external reports bind author mappings and failed
iterations. Original native diagnostic failures and operator-skipped Windows/clean-account
prerequisites remain unchanged. This source checkpoint adds no AX, guest, clean-machine or release result.

| Retained record | SHA-256 |
| --- | --- |
| Fourth final local result | `922b29f2441cc204dc248a2eb225768e713807f7ba6c3f8fefcffabf052ae465` |
| Fifth final local result | `0972acc2ace320520ab7c6e2ad1addb0acfd14a554dbdd4d558a2d043bd5380c` |
| Fifth hosted failure analysis | `4cf5707d0efdd1848b7107145088936fd578c74afa1962b92d0937008b39c508` |
| Storage admission report | `c2b4fe57b4d5ce187d942ce7f7a7c71711a4173d5870ce157aa10bd8a3983fdd` |
| Storage independent review | `88d52f39647202ce4ed0516e97aeb184248dede4e6831304e9581608cffb6541` |
| Onboarding and rendered landing report | `510e7d2b5d450163196ae7500850ed10b61c4406f154e3c065f82859f19f696c` |
| Onboarding independent source review | `3958d4e075e42b26dbb9ab2f100b0dfba5a0b0f66af9a31f19361dbee86211ce` |
| T12 fixture report | `b7a527b2cb8fed6931741629a9c63fdeca65223ca68e6f6265c457c9853bc88f` |
| T12 independent commit review | `1bd814b61a0b0c070721229c19b38f0f6d5123f49d314db31a54c1986eda220c` |
| Prior mixed-origin correction | `02e3783a9b83f5307dd92837b98a2f4eaa835a8f63eeb9e9dda9d072fe72da61` |
| Fresh GitHub-only reference review | `cf5fb7473ce7cb022de33f3f216ea4ea913c9f24b0a96eb65428b615efbf501f` |
| Fifth unchanged-binary diagnosis | `b37c36dda8941fefd3ab1e34e019ba8bf7b343dea980fd6d86e96681578be4b4` |
| Owned-child independent source review | `619a855befce3e61db0da178cfe46c4f409a400a16653f705f78a961f6c538d3` |
| Initially blocked combined source review | `8d7204f4aa2c09430ea946f161b5f7478f75941b4ba4461b202e39e3342af878` |
| Fixture ownership repair report | `64659eb37c5f70092f0f9269958283a5129b320e58ca35e293105e082120841a` |
| Fixture ownership independent review | `1c618ab11e11ca2a46f4f209435e6369fd09cf71f733bd6cbf60154ddc86fbcc` |
| Final combined source review | `da61ef72072dba6ce9334db506899d618513f6b0bd019ada2dd0c05aa838d9ed` |
