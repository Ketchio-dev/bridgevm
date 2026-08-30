# Licensing and attribution

This is the reference for what BridgeVM owns, what it uses, and what it ships.
[`THIRD-PARTY-NOTICES.md`](../THIRD-PARTY-NOTICES.md) is the distributed
document that carries the actual obligations; this page explains the position
behind it and the checks that keep it true.

## What BridgeVM's own code is

The virtual machine monitor is maintained as BridgeVM-owned Rust code against
Apple's Hypervisor.framework and published guest-interface specifications. That
scope includes the vCPU execution loop, guest memory map, firmware and device
tables, and the virtio, NVMe, xHCI, GPU, TPM and network device models.

This repository audit can establish what is present in the current tree and
what the build ships. It cannot provide a legal guarantee about every past
development session. Current BridgeVM work therefore treats other VMM source
as out of scope and derives owned behavior from public specifications,
BridgeVM's declared contracts, tests and live receipts.

Implementing a published interface is not derivation from a particular
implementation of it. BridgeVM implements the virtio specification, the NVMe and
xHCI register interfaces, the QEMU `virt` guest platform contract that Windows
on Arm firmware expects, and Apple's framework APIs. Documents should name the
interface being implemented rather than crediting an unrelated product for it.

The practical consequence for writing: describing behaviour as "the same
mechanism product X uses" reads as an admission of copying even when the code is
original. Name the API or specification instead.

## Provenance map

| Boundary | Examples | Repository treatment |
| --- | --- | --- |
| BridgeVM-owned implementation | HVF lifecycle, memory map, PCIe/MMIO dispatch, GIC, NVMe, xHCI, virtio transports, ACPI/SMBIOS generators | Apache-2.0 source in this repository; behavior is justified by public specifications, declared contracts and BridgeVM evidence |
| Required compatibility ABI | QEMU `virt` machine contract, `qemu,fw-cfg-mmio`, `QEMU`/`QEMU CFG` protocol literals | Names and byte values remain where bundled firmware requires interoperability; isolated in the machine contract and `fwcfg.rs`, not treated as source provenance |
| Modified third-party component | virglrenderer | Upstream licence and copyright retained; exact local patch and pinned upstream revision remain visible |
| Unmodified third-party component | EDK2, swtpm/libtpms, dynamically linked support libraries, guest drivers and Mesa payloads | Kept outside BridgeVM-owned code and distributed only with the licences and notices recorded in `THIRD-PARTY-NOTICES.md` |
| External compatibility executable | user-installed `qemu-system-*` | Launched only by the Compatibility Engine; not linked, embedded or redistributed by BridgeVM |

Changing prose does not establish code provenance. The enforceable boundary is
the tracked source, dependency graph, shipped artifact inventory, visible patch
set and repeatable licence checks.

## What is used but not written here

Third-party components are used as published binaries or libraries under their
own licences, and every one with an obligation is listed in the notices file
with its licence, linkage form and what that obligation is.

Only one component is modified rather than merely used: virglrenderer. Its local
patch is kept at
[`scripts/patches/virglrenderer-macos-venus.patch`](../scripts/patches/virglrenderer-macos-venus.patch)
and applies to the upstream commit pinned in
[`scripts/build-venus-host-deps.sh`](../scripts/build-venus-host-deps.sh).
virglrenderer is MIT-licensed, its copyright notices are retained, and keeping
the patch in the tree means the modification is visible rather than folded into
a shipped binary.

## What is deliberately not shipped

- **Guest operating systems.** Windows media is the user's own, used under their
  Microsoft licence. No OS image is tracked in the repository; the `ISO/` path is
  a developer convenience excluded by `.gitignore`.
- **QEMU.** The Compatibility Engine is a real product lane, but it plans and
  launches a `qemu-system` binary the user installs. No QEMU code is linked,
  embedded or shipped, so no QEMU obligation attaches to the distribution.
- **Apple D3DMetal / Game Porting Toolkit.** Its licence prohibits
  redistribution and non-evaluation use.
- **DXMT.** Not currently shipped. If a future release bundles it, LGPL requires
  it to remain a separate dynamic library with its source published.

## Copyleft and linkage

Rust dependencies are statically linked, so the `deny.toml` allowlist is
permissive-only. Admitting a copyleft licence there would create a
source-distribution obligation the distribution does not otherwise carry.
Nothing in the current closure is copyleft.

The LGPL components that do ship are the swtpm dependency closure. Each is a
separate `.dylib` under `Contents/Frameworks/`, linked dynamically so a user can
replace it, which is what LGPL requires.

## How this is enforced

| Check | What it proves | Needs |
| --- | --- | --- |
| `scripts/check-attribution-honesty.sh` | The notices file still answers the provenance and guest-OS questions; no tracked OS image; no source or document claims derivation from another VMM; the virglrenderer patch is present and referenced; promised licence texts exist; `deny.toml` admits no copyleft licence | the tree |
| `scripts/verify-app-third-party-notices.sh` | The bundled licence and notices match the repository byte for byte; every LGPL dylib is a separate Mach-O with a proven dynamic consumer; no static archives; the Rust inventory and licence-text bundle agree | a built `.app` |
| `cargo deny check` | Every dependency licence is on the permissive allowlist | the lockfile |
| `scripts/verify-rust-dependency-inventory.sh` and `scripts/verify-rust-license-bundle.sh` | The shipped inventory and licence texts cover the same package set | a generated inventory |

The first three run in `scripts/check-project.sh` or hosted CI. The bundle-level
checks run during release packaging, because they need an actual app.

Each rule in the attribution check is mutation-verified: removing a required
notices section, dropping the virglrenderer patch reference, re-adding a
copyleft licence to `deny.toml`, or adding a derivation claim to a source file
each make it fail.
