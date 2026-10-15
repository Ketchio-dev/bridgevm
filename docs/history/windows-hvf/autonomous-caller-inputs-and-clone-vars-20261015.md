# Caller input checks and independent clone vars — 2026-10-15

This follows the [diagnostic attribution record](autonomous-diagnostic-attribution-20261014.md).
It records two bounded source conclusions from automated tests and static review.
The resulting eighth candidate still requires its own full-project and exact hosted checks.
Product state, capability wording, criteria, thresholds and known defects remain unchanged.

## Earlier results retain their exact scope

The fifth checkpoint remains failed: its local full check recorded 44 PASS / 1 FAIL
at child-fixture readiness, with the cause unknown. Its separate hosted manual CI
failed during T12 temporary-Git cleanup. Later passes do not erase either failure.

Sixth source `df70acf596237a0ed99e15e83ce197d4df00a96b` passed all 45 local stages
with 5,101 tracked files unchanged. The captured hosted cohort reports all 103 associated
workflow runs successful: 170 successful jobs and two explicitly skipped jobs.
Its [full hosted run](https://github.com/Ketchio-dev/bridgevm/actions/runs/38654355897)
checked out exact sixth source and passed all 44 executed stages; the unavailable optional
Linux target was skipped, not counted as an executed stage. Independent review now
authenticates those deterministic results; it does not authorize a live operation.
API head association does not assert identical actual checkout commits for every PR job.

Seventh source `d3cace7877ae7c0e1781d36e1ae77ed8e7eb6040` ran from October 14
23:59:50 to October 15 00:23:35 UTC and finished with 44 PASS / 1 FAIL across 45 stages.
All 5,114 tracked files remained unchanged. The sole failed stage was shell checking:
four SC2154 findings in two newly sourced checker files. The four shim suites passed
425 / 1,408 / 65 / 379 methods, with three existing Control skips. Seventh remains failed.

## Sourced checks require their caller inputs

Integrated [f8bbd303349b](https://github.com/Ketchio-dev/bridgevm/commit/f8bbd303349b3c0f46a7ca99a258575ae9ea3a28)
adds two explicit required-variable guards to the sourced AX contract checkers.
It changes neither the shell gate nor its check list, suppresses no warning, and retains
the existing 10- and six-line ceilings. Statements after the replaced comment are unchanged.
The exact baseline checker reproduced four findings and exit 1; the repaired official
shell gate passed all 532 scripts. A paired argument recorder received the same five calls
before and after the change. Seven missing-input controls refused before compilation.
The recorder establishes shell routing, not a new Swift, native UI or AX execution result.
The original full failure remains recorded; this focused pass does not replace it.

## Clone copies must not share writable vars through links

Integrated [59c72c7c66ec](https://github.com/Ketchio-dev/bridgevm/commit/59c72c7c66ec454c280135129ee2767bd8ca91f4) extends the existing clone-source
shape checks to the metadata directory and UEFI vars file before destination reservation.
The concrete baseline used legacy migration and the actual LibraryModel request/action
admission paths, with queued UI jobs captured and discarded. Separately, the real recursive
copier preserved a vars-file symlink and a metadata-directory symlink. In both cases the
copied vars resolved to the original target's device/inode; ordinary copied vars had a
distinct identity. The copier first attempts `cp -c -R`, then uses FileManager recursive
copying on failure. The recorded identities do not identify which branch succeeded.
Three baseline methods passed one and failed two, with 14 failed assertions.

The repaired shape check requires ordinary internal directories and regular disk/vars files
at copy time. Links, wrong entry types and metadata errors refuse. The existing admission
policy for missing entries remains: the UI may schedule them, but copying requires the full
entry chain before reservation. Selected-disk equality and the copy guard order are unchanged.
Final native checks passed 56 methods; the actual shim passed the same eight new tests with
zero failures or skips. Missing metadata/vars, wrong entry types and dangling links are covered.
The actual copy callback stopped before key custody and registration, and synthetic source
bytes and identity remained intact. No completed clone publication or VM execution was tested.

The original failed binary and logs remain. The final fixture uses exclusive private-root
creation; replaying the original guard with that fixture retained the same 3-method,
14-assertion failure. An earlier repaired 56-method pass used three older temporary-directory
fixtures without explicit SSD resolution; the final pass follows their venue correction.
The failed patch application changed no files and is retained separately. Existing warnings
remain in the raw logs. Prior ceilings only decrease; new modules use their actual sizes.
No hostile concurrent replacement, cross-library coordination, Keychain, native lease,
guest-media, GUI or guest-behavior guarantee is added. The independent review and exact
committed-source binding are recorded below; integration validation remains separate.

## Evidence bindings

The table omits private paths; retained external reports hold the source mappings, raw
failures and focused proof. No operator-skipped prerequisite or release criterion is closed.

| Retained record | SHA-256 |
| --- | --- |
| Sixth independent local full verification | `7d2fd648dcd5ecbf4701ce923ee5917468430cd2d5540ee4715b6ae515fd698e` |
| Sixth hosted terminal capture report | `122022d536a00b8b7c305033d98ede3051cddee29310031656af1ec923426f55` |
| Seventh independent failed-full verification | `bc57bf3239cb2f8bf41bef091ed9a7081277dadca8a1b6a53743a41b63c192c2` |
| Caller-input guard report | `fd915435d0f290407b052c4a75caaeda4f4530c63734c67c58715b28b52686e3` |
| Caller-input guard independent commit review | `3c194cdf95b06eaff7112fa4e3cf63a333a2f23099b79345b0498d46759a97b8` |
| Clone vars focused report | `7721df5b47a007986ffd1397668f90c93874dcfac1a07f289e4d902dd0024999` |
| Clone vars source/proof receipt | `6bf7f65969a63ed4187b73273e6cdebbe4969f564c0d16b769ac6fb26f5b5296` |
| Sixth independent exact-source review | `4e60d0f6fc6299e28a75dc6251b718e64afce5818034c5caf5341a9cc8bda71d` |
| Clone vars independent commit binding | `502063a63f0d154675bf591630b534bb19bf42cfd234616491cc9cff7631fad5` |
