# Selected-media relocation follow-up — 2026-10-14

Evidence rank: **test**, with a separately identified source-only wording repair.
This eighth batch follows [transfer preservation](autonomous-import-preservation-20261014.md)
and [interrupt/fixture investigation](autonomous-interrupt-fixtures-20261014.md).
No guest capability, product state, live sample count or criterion is promoted.
Earlier failed experiments retain their original outcomes.

## Keeping the selected disk usable after moving its bundle

A migrated registration can select an external symbolic link whose target is
inside the VM bundle. Relocation moved the bundle and reported success but saved
the unchanged external alias, which then pointed at the old, absent location.
The public relocation API baseline ran three methods: builtin and independent
external-disk controls passed; the alias-to-internal case failed one assertion.
A separate actual native-media-lease baseline failed two assertions in one method.
The original mapping was restored for that baseline, with the new helper unused;
lease acquisition, move and release were real operations on tiny owned fixtures.

Relocation now resolves the selected disk and source bundle components before
moving. A selected target contained by the source is mapped to the corresponding
destination-relative path. Other selections retain the existing lexical fallback.
Independent external paths and aliases remain unchanged, and no external link
entry is rewritten. Only saved diskPath mapping changes; source-reference,
destination, transaction, ownership, journal and lifecycle checks remain.

Final native and the same actual-shim selection each passed 50 methods, with
zero failures and zero shim skips: ten new controls and forty related methods.
These overlapping selections must not be added as 100 distinct tests. They cover
implicit and explicit builtin disks, external aliases into two internal targets,
bundle-ancestor aliases, independent external direct/alias targets, same-name-
prefix siblings and an actual lease-protected move. Disk, vars and external
source hashes remain checked before and after the move.
The same 50 native and 50 actual-shim cases also passed under a short owned
aliased TMPDIR, with source and all three executable hashes unchanged; no rebuild.

Two final controls only stat existing system directory aliases and exercise pure
path mapping. They move, create or write no system path. The Foundation probe
found no normalization mismatch for the tested spellings; expected destination
assertions remain exact. All generated data and builds use owned scratch.
Earlier 46-, 47- and 48-method passes remain recorded. Adding source-hash logging
initially failed compilation because of a missing try; that source and log remain
retained, without a behavioral result. The final 50-method selection is authoritative.

The actual lease test reuses a historical helper, SHA-256
`d92bd21d5017c3b899719bc2e1c1dfacd57afa481ccade92f8fbc85af171617c`.
Its provenance is the earlier source plus approved restore patch; 51 relevant
lease/managed-pair/media source files match current source. It is not a newly
built current-whole-head helper. Final shim tests were freshly compiled against
unchanged production/XCTest modules from the earlier 48-method build; module
hashes and unchanged source inventory are retained separately.

This is point-in-time path rebasing, not an inode-lifetime, hostile concurrent
rename or arbitrary alias-topology guarantee. No real Keychain operation, vTPM
state, canonical guest media, guest boot or live gate was used. Constructing the
key-store object on the no-state same-identity move path does not invoke it.
Adjacent export/deletion reading demonstrated no additional defect and supplies
no new runtime proof for those workflows.

## Qualifying APFS clone copy wording

One line in the native clone sheet now says APFS copy-on-write is used when
possible and ordinary copying is attempted otherwise. Source inspection shows
the clone operation calls a helper that tries cp -c -R, then FileManager.copyItem
on failure. The revised wording describes an attempt and does not promise success.

This is a source-reviewed text correction only. Admission, copying, registration,
TPM and evidence-retention logic remain unchanged. The existing 31-line ceiling
is unchanged. No new test, build, browser replay or cross-volume execution was
performed for this one-line conclusion; its result must not be called a tested
cross-volume clone guarantee.

## Parent check and bound evidence

The preceding exact parent `f5da1e7056eac0e0a9322d4901311a0a4965c9fb`
completed scripts/check-project.sh locally from 07:45:26 to 08:05:46 UTC on
2026-10-14: all 45 named stages passed, exit zero, with source unchanged.
Its log SHA-256 is
`c6ab940b0fe2e97f137b42b890462fde7c888ef00093a735c7a7cdeb4beb2b0e`.
That result belongs to the parent, not this successor. Hosted confirmation was
pending when this checkpoint was drafted. The integrated successor still needs
its own required local and exact-SHA hosted receipts; no release claim follows.

The following retained receipts bind source inventories, patches and scoped
proofs. Existing structural ceilings are not increased; the relocation helper
and two test files are registered at their actual sizes. No private filesystem
paths or generated payloads are included here.

| Artifact | SHA-256 |
| --- | --- |
| Relocation final v2 report | `13b2455b91a91b274a480e24fb71dbcfaec9267e11130fc950c0c0c0504e1446` |
| Relocation v2 patch | `0dc4cf886c04a792ab688e544bde7ddefeb4483f282b03796df2a3be52fd2a7b` |
| Relocation independent review | `931bd79b0500e89fd718f071f6192b9176f2b701dcffd8f500bdf28e0c5790bb` |
| Relocation same-binary alias replay | `991dac34de8b31c9107e4ea2403aafddafbc393f45ccd0f1f85b5e7964ceed7b` |
| Clone wording receipt | `bb6007f3e6bb978edf154777d9ee339f389de418e087887c38f75cc16e45277b` |
| Clone wording patch | `5fb126d1e539fcaf364fc103e46e8e00ebd7b79da358d42dd9ea5f29c4cf7bee` |
| Clone wording independent review | `4f974c5530a89b801cde36c4f17bfb16df2a94b976053b26ce31a1aadf0698f0` |
