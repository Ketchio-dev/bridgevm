# Windows kernel-policy package provenance

Document status: **Current**

Last reviewed: 2026-08-25

Windows-HVF driver injection may consume only a finalized package whose Windows
kernel-policy status and complete file inventory are covered by a BridgeVM
Ed25519 signature. An Authenticode-looking file, a package-local boolean, a test
certificate, or a successful deterministic verifier test is not sufficient.

## Trust anchor

The product currently pins this public anchor in
`HvfWindowsKernelPolicyVerifier.swift`:

- key id: `bridgevm-kernel-policy-2026-01`
- algorithm: Ed25519
- raw public key (base64):
  `JoFvcf9P9qvU3tvW7DPCGFEUmo713CORBOagzSmG2GA=`
- raw public-key SHA-256:
  `e92779e944b53ce31a8a4e84d26777629917d3e96de279bce7f4c4c0a3bea96e`
- activation: `2026-08-25T00:00:00Z`
- expiry: `2028-08-25T00:00:00Z`
- status: active, not revoked

The private key is a mode-0600 operator secret outside the repository. It must
never enter git, CI, an app bundle, a Studio receipt, diagnostics, guest media,
or a release artifact. Possession of that key authorizes a package, so signing
is a release operation, not a developer convenience.

## Signed envelope

`scripts/sign-hvf-windows-kernel-policy-package.py` creates a new output
directory and never changes the finalized source directory. It requires a flat,
regular, non-symlink package and a report that says all of the following:

- finalization completed;
- signing mode is `kernel-policy`;
- TESTSIGNING is not required;
- both SYS and CAT passed the Windows kernel-policy verification step;
- each final artifact hash matches the report.

The canonical JSON attestation binds schema and policy versions, key and package
ids, issue and expiry times, and the exact sorted file-name/SHA-256 inventory.
The finalization report itself is in that inventory. The detached `.sig` is a
64-byte Ed25519 signature over the exact canonical JSON bytes.

An attestation is valid for at most 366 days and must fit wholly inside its
anchor's validity interval. The verifier rejects noncanonical or oversized
input, unknown JSON fields, duplicate report fields, unknown or revoked keys,
bad time windows, invalid signatures, path-like or case-colliding names,
symlinks, subdirectories, extra or missing files, and every hash mismatch.

## Rotation and revocation

Routine rotation adds a new key id and public key in a product update before the
old key stops signing. During the overlap, each attestation still names exactly
one key. New packages switch to the new key; a later product update removes or
expires the old anchor.

Compromise response does not wait for expiry. A product update marks the key
revoked, and revocation overrides an otherwise correct signature and date. Any
package signed by that key is then refused. Re-authorization requires a new key,
a new attestation, a newly verified package snapshot, and new release evidence.

## Mutation boundary

Verification of a user-writable source path cannot authorize later mutation.
The product snapshot operation first verifies the source, copies only the signed
inventory to a private temporary sibling, verifies the copied bytes again, and
then atomically renames that directory into place. A source replacement or
copy-time mutation therefore produces no accepted snapshot.

The verifier and snapshot primitive are deterministic security prerequisites,
not A9 completion evidence. Product injection remains unavailable until install
and import consume only such a snapshot, a real Microsoft kernel-policy package
passes the flow, and one retained clean-machine receipt proves the full path.
