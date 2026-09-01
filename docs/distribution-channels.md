# BridgeVM distribution channels

Document status: **Current**

Last reviewed: 2026-08-29

BridgeVM separates the app that ordinary technical users can try from the
Windows driver work that still requires a development security posture. The two
channels do not share an artifact or an implied support promise.

## General Preview

The General Preview is the only user-facing distribution channel.

Its contract is:

- product state is **Engineering Preview**;
- the app contains no Windows driver package (`.sys`, `.cat`, `.inf`, or
  certificate/private-key payload);
- Windows-HVF ISO install and installed-disk import are 3D-off;
- the app does not enable Windows TESTSIGNING or weaken Secure Boot policy;
- users bring their own licensed Windows 11 Arm ISO;
- ISO installation also requires a user-supplied signed ARM64
  storage/serial/network payload and external SHA-256 manifest; these inputs
  are sealed into the VM bundle but are not redistributed in the app;
- the Mac app is ad-hoc signed, not Developer ID signed or notarized;
- the release carries `BridgeVM-release.json` and `SHA256SUMS` beside the app
  archives.

The terminal installer accepts only a non-prerelease carrying a valid
`general-preview` contract. The manifest must state that no Windows kernel
driver is included, test signing is not required, product injection is
unavailable, and install mode is `3d-off`. A checksum-valid archive with the
wrong channel is still refused.

The historical `v1.0.0` predates the current A9 fail-closed product boundary.
It remains evidence of what was released, but it is not the recommended General
Preview and has no channel manifest. The installer therefore refuses it.

## Graphics Lab

Graphics Lab is for contributors investigating the experimental virtio-gpu 3D
path on isolated test VMs. It is not a second consumer edition of the app.

Its contract is:

- test-signed packages are experiment inputs, never production-signing
  evidence;
- every package is identified by an immutable source revision and file/tree
  hashes;
- TESTSIGNING and any different Secure Boot posture are explicit operator
  choices, never automatic fallback behavior;
- canonical Windows images remain immutable, and each lane uses its own cloned
  disk and UEFI vars;
- test guests should contain no personal credentials or irreplaceable data;
- results apply only to the exact sealed package, image, vars, manifest, and
  BridgeVM commit that the receipt names.

The retained B4 campaign passed its fixed 20-lane pointer gate at 20/20 with
p95 243 ms and no rendering/package regression lanes. That supports the exact
experimental package tested in the receipt. It does not establish Microsoft
kernel-policy signing provenance and therefore does not close A9. See the
[B4 receipt](windows-arm/evidence/b4-pointer-reliability-proven-20260830.md) and
[capability matrix](windows-arm/capability-matrix.md).

Microsoft documents test-signing as a development/test mode and requires
Hardware Dev Center signing for public-release kernel drivers on current
Windows. See Microsoft's
[kernel-mode signing requirements](https://learn.microsoft.com/en-us/windows-hardware/drivers/install/kernel-mode-code-signing-requirements--windows-vista-and-later-)
and [test-signing documentation](https://learn.microsoft.com/en-us/windows-hardware/drivers/install/the-testsigning-boot-configuration-option).

## Promotion rule

Graphics Lab does not become General Preview merely because a workload passes.
Promotion requires all of the following:

1. a Microsoft kernel-policy-signed ARM64 package with retained provenance;
2. the same verified package identity consumed by both ISO install and disk
   import before either VM is mutated;
3. a retained clean-machine product-flow receipt;
4. A9 promoted from the machine-readable capability registry without changing
   its criterion;
5. exact-head hosted CI, Security and quality, packaging, attribution, and final
   project checks.

Until then, the general artifact stays 3D-off and A9 stays OPEN.

## Release operator checklist

Before publishing a General Preview draft:

1. confirm the release commit is the intended clean tree;
2. run `scripts/check-project.sh`;
3. require exact-head hosted CI and Security and quality to pass;
4. run the release workflow in dry-run mode and inspect
   `BridgeVM-release.json`, `SHA256SUMS`, the DMG, and the tarball;
5. confirm the app bundle contains no `.sys`, `.cat`, `.inf`, `.cer`, `.pfx`,
   or `.p12` files;
6. test the terminal installer with `--dry-run` and then on a clean Mac account;
7. exercise Windows install from a user-supplied ISO and sealed signed ARM64
   storage/serial/network payload with 3D injection absent;
8. review release notes against `capabilities/windows-hvf.json` and publish only
   the generated capability wording;
9. preserve the artifact hashes and workflow run URL with the release record.

Building a draft is not permission to publish it. Any failed step leaves the
draft unpublished.
