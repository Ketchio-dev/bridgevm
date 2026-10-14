# Transfer preservation follow-up — 2026-10-14

Evidence rank: **test**. This seventh batch follows
[the reservation checkpoint](autonomous-reservations-20261014.md).
Its [interrupt and fixture companion](autonomous-interrupt-fixtures-20261014.md)
records device notification repairs and the separate failed-check investigation.
These are focused lane results; counts overlap existing suites and must not be
summed as unique tests. Integrated local and exact-SHA hosted checks require
separate receipts. Earlier failures remain failed, with no live criterion,
capability or product-state promotion.

## Preserving occupied tar-export destinations

Public CLI and `VmStore` tar export could follow a preexisting dangling output
symlink into an absent file inside its own source bundle, mutate that source,
and return success. Destination admission now uses link metadata; only NotFound
means absent. Tar creation is exclusive, so a file or link appearing after
admission also refuses. Existing containment resolution and source leases remain.

The unchanged eight-method baseline passed six and failed two; all eight passed
after repair. Eleven storage export tests passed, including four new controls.
Directory export refusal, existing tar bytes and healthy export/import roundtrip
remain covered. Four concurrent creators using distinct source bundles produced
one winner; inserted file/link collisions, partial-file retry preservation and
non-directory metadata errors were also checked. No compilation or repaired-run
failure was recorded for this lane.

These are tiny synthetic files. The repair does not add output cleanup, atomic
archive publication or authenticated namespace replacement. Newly created partial
output retention is unchanged. A retained partial-file retry is tested; an
injected mid-write failure, real crash or permission-denied case is not.

## Refusing a clone that would copy the wrong selected disk

Legacy migration can preserve an external native disk selection. Runtime and
media leases honor that path, while bundle cloning previously copied and selected
a different bundled disk. The actual copy baseline passed its managed control
and failed the external-selection case after inspecting copied bytes. Its hook
then refused before Keychain or publication. The actual UI admission baseline
failed six assertions in one method: only original admission was restored while
the copier repair already existed, and scheduled jobs were prevented from running.

Clone sheet/action admission now refuses an unsupported selected-versus-copied
disk mismatch with import guidance. The copier independently validates direct
bundle/disks directories and a bundled regular disk before reservation, callbacks
or key custody. Direct symlinks and present wrong types refuse. A selected alias
resolving to the same bundled regular file and aliases in bundle ancestors remain
accepted. The UI's existing NotFound-only missing-media scheduling policy stays.
No external-disk cloning feature or arbitrary hardlink-equivalence rule is added.

Final native and the same actual-shim selection each passed 61 cases, including
13 new methods, with zero shim skips. Source bytes, vars, registration and link
target preservation are asserted. Earlier failed fixtures remain: placing assets
inside the library triggered migration refusal; a missing displayName argument
failed compilation; and three existing activity cases failed nine assertions in
a 45-method run because their positive disk path was not the builtin path.
The corrected fixture still shares the real builtin path with its generic peer;
all activity, refusal and recovery assertions remain.

This is point-in-time path admission. No hostile concurrent replacement,
real Keychain operation, guest boot, WindowServer or sanitizer result is claimed.

## Refusing deletion-marked imports without erasing their records

Import preserved deletion metadata but reported success: valid records hid the
newly occupied destination from lookup/list/status, while malformed records made
inventory fail even when an unrelated healthy VM was already registered.
The policy-neutral baseline allowed either usable success or clean refusal.
Whole-bundle import documentation does not promise undelete; the visibility
invariant comes from existing successful-import lookup tests and this observed
inconsistency. An earlier broader attribution to import prose was corrected.

The coordinator chose refusal as a reversible autonomous default after an
optional operator preference went unanswered. This was **not user approval** of
a recovery policy. Any deletion marker now refuses with an explicit reason;
records are neither parsed into recovery nor silently cleared. Only NotFound
means absent, other metadata errors propagate, and the owned copied stage is
checked again before publication. Existing lease, copy verification and cleanup
remain. A future explicit recovery design may supersede this default.

The final identical neutral eight-method fixture passed four and failed four on
original production, then passed all eight after repair. Two malformed-marker
cases both failed before repair and passed afterward. Four additional no-rename
CLI directory/tar controls passed: the final CLI/API selection is 14/0. The
storage import selection passed 26, including valid/malformed/null/empty records,
directories, dangling links and FIFOs with and without rename, plus non-directory
metadata error propagation and owned-stage rejection/cleanup. Healthy imports,
repeat refusal, source/archive bytes and existing inventory remain checked.

Failed experiments remain: an initial SHA256-formatting fixture compile error;
the original four-failure and two-failure baselines; and a 25-method storage run
with one socket fixture failure at the platform pathname limit. FIFO replaced
that special-file fixture without a production change. A mutex-only correction
serialized extraction-namespace observations; final unchanged test bytes were
replayed against original production before the repaired default-thread run.
Tar tombstone fixtures append authentic deletion records to an earlier healthy
export; they are not public exports of already hidden registrations.

No recovery tool, repair of previously poisoned stores, synchronized import-race,
crash, hostile namespace or live guest claim follows from these tests.

## Bound evidence

Independent reviews bind the patches, source inventories and retained logs.
Full SHA-256 values below identify the reviewed artifacts; private storage paths
and generated payloads are omitted. Structural ceilings only ratchet downward.

| Conclusion | Reviewed patch SHA-256 | Independent review SHA-256 |
| --- | --- | --- |
| Tar export | `284984f0e01111a7ab7549d7b15a7ce691b2e3d95e746d81e84135e382fbb1fd` | `31e45040aabf88cd4810771cf308b9ded5a62917e35e38b9772f4dcf9453fa7b` |
| Selected-disk clone | `7d974d2db6b604167c8b353488196193302dc1314e6a8693542273094fb1318c` | `696962592fe7e9204ffbb9f9e3ee77c870ea494e9b452076883f98760e7cbc8d` |
| Deletion-marked import | `c974c90c952dd0eff1fe49ef7811883480bc82b908efd83deb413a3b3440b306` | `64d8fb6bd28d26d642a91297c0794272d90352cd7537fb405f08f89ea247825a` |
