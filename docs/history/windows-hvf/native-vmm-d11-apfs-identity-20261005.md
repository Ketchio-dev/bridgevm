# D11 owned APFS mount identity — 2026-10-05

Integrated source checkpoint `bf62c202985647f29f595dbab6451fa65c2ab4b8`
repairs an owned-container metadata mismatch. This is development fixture
preparation, not a Windows installation or release-criterion result.

## Reproduced predicate and preserved failures

A real empty-container experiment using old 15f production fails with
`container is not its owned APFS filesystem`. Its retained ATTACH plist reports
the APFS leaf, while current INFO omits `volume-kind` for that same leaf. The
old predicate incorrectly requires that ATTACH-only field on INFO. Negative
experiment result SHA-256:
`6b17896791f3ba16f820ce0c16bf3d9999f722759abc41b948e479bdd37d7cac`.

This identifies the guard failure in that controlled empty experiment only.
Original D11 R1 was canceled before fixture output; its blocked-open cause
remains UNPROVEN. Original D11 R2 later records a post-attach ValueError, but
its exact predicate was UNRECORDED. The similar boundary does not retrospectively
prove that R2 had this cause. Both failed attempts remain preserved.

## Strict identity binding and deterministic proof

`scripts/live-gates/d11_fixture_mount_identity.py` binds a bounded, stable
ATTACH plist's explicit APFS mount and leaf to the same current INFO backing,
mount, leaf and whole-device identity. Both surfaces must have unique mount
and leaf associations. An INFO type, when present, must still be APFS. Missing
ATTACH type, malformed or aliased metadata, foreign devices and ambiguous
associations fail closed. Existing filesystem-device and capacity guards remain.

Eight unchanged paired methods cover 27 cases. Corrected baseline fixtures
expose 18 failing negative subcases and one positive-case error; repaired
production passes all 27 cases. An initial unsupported `None` value in the plist
fixture remains recorded and was corrected before this paired comparison.
Corrected baseline stderr SHA-256:
`fa882022d76a7f324677da5eb05d4f40e09b175a39d75e423d0140ad5dd8dca2`;
repaired stderr SHA-256:
`432273a93acad2277d6e8f9c68d101a7299efecbbd73fc3c5f00db371e281fcb`.

Related checks pass 61 Python methods, the native-helper contract and seven
mocked Windows readiness/refusal/launch cases. Independent source and retained
evidence review does not replay the tests. The existing mount module ceiling
decreases from 110 to 97; the new identity, contract and support files are
registered at actual counted sizes 46, 93 and 78.

## Owned empty-container follow-up

The repaired production create path succeeds in a separately owned empty
host-filesystem experiment, followed by normal cleanup. Both recorded create/
attach command groups exited 0 and were absent at observation; cleanup returned
true. Actual duration is 11.96189158 seconds. Result: 944 bytes, SHA-256
`cff29c583125e3971bcf6db3173f4362009723cc7395dd6a75b89e7362807f91`.

Measured filesystem capacity is 23,412,563,968 bytes, within the unchanged
23,622,320,128-byte limit. Host free bytes are 138,974,830,592 before and
138,958,880,768 after. The 22 GiB logical cap, 128 GiB minimum free admission
and 104 GiB reserve are unchanged. No VM, private guest asset or queue job was
used, and earlier experiment backing was untouched. This is a finite observation.

## Evidence boundary

Successful empty APFS preparation does not prove Windows installation, real
guest storage fit, T15 readiness or any release criterion. The combined
successor still requires complete local project and exact-SHA GitHub-hosted
checks. All 29 criterion states, thresholds, known defects and product wording
remain unchanged; no guest performance or release promotion is claimed.
