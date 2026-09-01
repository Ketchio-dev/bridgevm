# Third-Party Notices

BridgeVM's own source code is licensed under the Apache License, Version 2.0;
see [`LICENSE`](LICENSE). The components below retain their respective
third-party licenses and notices. Copyleft components are distributed only in
the separately replaceable or standalone forms described below.

## Provenance of BridgeVM's own code

BridgeVM's virtual machine monitor is maintained as BridgeVM-owned Rust code
against Apple's Hypervisor.framework and published guest-interface
specifications. That includes the vCPU execution loop, guest memory map,
firmware and device tables, and the virtio, NVMe, xHCI, GPU, TPM and network
device models.

Where BridgeVM implements a published interface — the virtio specification, the
NVMe and xHCI register interfaces, the QEMU `virt` guest platform contract that
the current firmware expects, or Apple's framework APIs — protocol names and
required byte values are kept for interoperability. BridgeVM-owned behavior is
maintained from public specifications, declared contracts, tests and live
receipts; other VMM source is outside that implementation boundary.

This notice describes the current source and distribution boundary. It does not
claim that wording alone proves historical code provenance. The repository's
tracked source, dependency graph, visible patch set and packaged licence
inventory are the auditable record.

The third-party components listed below are the parts BridgeVM genuinely does
not write. Some are used unmodified and some carry clearly identified BridgeVM
patches; none is relabelled as BridgeVM-owned code.

[`THIRD-PARTY-PATCHES.tsv`](THIRD-PARTY-PATCHES.tsv) is the machine-readable
patch inventory. It records every tracked patch, exact upstream revision,
licence, licence-text and patch digests, and one of three distribution scopes:

- `product-bundled`: the patched result is present in a shipped app or firmware
  image. This applies to the main virglrenderer patch and the TianoCore EDK2 TPM
  PPI patch.
- `graphics-lab-artifact`: the patched result may appear in the Windows UMD CI
  artifact or Graphics Lab package, but is absent from the General Preview.
  This applies to the two Mesa patches.
- `internal-validation-only`: the patch is retained for diagnostics or live
  validation and is not applied to a product or public CI binary. This applies
  to the DXVK relaxation patch and the older virglrenderer draw-probe patch.

The product firmware build receipt contains the exact EDK2 patch digest. The
product renderer build applies only the registered main virglrenderer patch.
The Windows UMD CI artifact and every staged/finalized Graphics Lab package
carry Mesa's upstream licence overview, a complete upstream `licenses/`
archive, the exact virtio-win driver licence, the two BridgeVM patches and a
digest-bound modification record beside the resulting binaries.

## Host-side (macOS app / VMM)

| Component | License | Linkage | Obligation |
|---|---|---|---|
| virglrenderer (freedesktop.org, with the registered product patch) | MIT | dynamic library (`libvirglrenderer.dylib`), loaded by the VMM process | ship license text, keep copyright notices and the visible patch |
| MoltenVK (KhronosGroup) | Apache-2.0 | dynamic library loaded at runtime (`BRIDGEVM_VULKAN_LIB`, default `/opt/homebrew/lib/libMoltenVK.dylib`); not distributed in-app by default | if bundled: ship LICENSE + NOTICE |
| Vulkan-Headers / venus protocol headers | Apache-2.0 / MIT | build-time headers | none beyond notice |
| Locked Rust crate dependencies | licenses recorded per package (permissive allowlist enforced by `deny.toml`) | statically linked into BridgeVM executables as permitted by each package license | ship the generated `rust-dependencies.tsv` inventory and applicable attribution |
| swtpm + libtpms (vTPM) | BSD-3-Clause | bundled helper binary + dylibs | ship license text |
| wimlib-imagex 1.14.5 | GPL-3.0-or-later | separately executed bundled command-line helper; not linked into BridgeVM | ship the exact GPL text, complete corresponding source archive and build recipe beside the binary; preserve the user's right to replace and redistribute it |
| GLib / GObject / GIO / GModule, json-glib | LGPL-2.1+ | bundled **dynamic** libraries (swtpm dependencies), replaceable by path | ship license text + LGPL text; keep dynamic linkage |
| gettext runtime (libintl) | LGPL-2.1+ | bundled dynamic library | same as GLib |
| OpenSSL (libcrypto) | Apache-2.0 | bundled dynamic library | ship license + notice |
| PCRE2 | BSD-3-Clause | bundled dynamic library | ship license text |
| libepoxy | MIT | bundled dynamic library | ship license text |
| TianoCore EDK2 secure firmware volume (with the registered TPM PPI patch) | BSD-2-Clause-Patent | bundled firmware image (`Resources/firmware`) | ship license text and exact build/patch receipt |

macOS system frameworks (Hypervisor, AppKit, Security, AudioToolbox, OpenGL,
libSystem, libiconv, libobjc) are Apple system libraries used under the macOS
SDK terms; they are not redistributed.

## Guest-side (Windows driver / tools payload)

| Component | License | Form | Obligation |
|---|---|---|---|
| viogpu3d KMD (arehnman/kvm-guest-drivers-windows `4c27e477e6560cea724d848b98149f03cb1f2083`) | BSD-3-Clause | signed `.sys` + INF in a Graphics Lab driver package; absent from General Preview | ship exact licence text, keep copyright notices and source revision |
| Mesa (Venus/Neptune/VirGL guest UMDs) | per-file; the BridgeVM-patched files are MIT | guest DLLs in Graphics Lab or internal validation; no driver payload in General Preview | ship the upstream licence overview and complete licence archive with any distributed binary, identify the exact patches |
| NetKVM (virtio-win) | BSD-3-Clause | guest network driver | ship license text |
| DXVK (BridgeVM relaxation patch) | zlib/libpng | title-local D3D11 validation DLLs only; not shipped in the product or public CI artifacts | retain the zlib notice and plainly mark the altered source; if distributed later, bundle the notice |
| Legacy TianoCore EDK2 `edk2-aarch64-code.fd` | BSD-2-Clause-Patent | development-only compatibility/regression artifact in the source tree; not installed by the macOS product packagers | exact original build revision was not recorded; retain the EDK2 licence and never promote it as a product artifact |
| PPSSPP (validation payload only) | GPL-2.0-or-later | standalone guest **application**, not linked with BridgeVM; used only in internal validation gates, **not shipped** in the product image | none if not distributed; if ever distributed, ship complete corresponding source |

## Guest operating systems

BridgeVM does not redistribute any guest operating system. Windows media is
supplied by the user and runs under the user's own Microsoft licence; BridgeVM
ships no Windows installation media, no Windows components, and no Microsoft
product keys. The repository contains no operating-system image: the `ISO/`
path is a developer convenience that is excluded by `.gitignore`.

## Explicitly excluded

- **Apple D3DMetal / Game Porting Toolkit**: license prohibits redistribution
  and non-evaluation use — NOT used, NOT shipped.
- **QEMU**: not redistributed. BridgeVM's own Windows path uses its
  Hypervisor.framework VMM and does not involve QEMU. The Compatibility Engine
  does drive QEMU, but only by planning and launching a `qemu-system-*` binary
  the user installs themselves; no QEMU code is linked, embedded, or shipped in
  any BridgeVM artifact, so no QEMU licence obligation attaches to the
  distribution.
- **DXMT (LGPL-2.1+)**: not currently shipped. If a future release bundles it
  for the D3D11-on-Metal host renderer, it must remain a separate dynamic
  library with its source (including modifications) published, per LGPL.

All LGPL components above are shipped ONLY as separate `.dylib` files under
`Contents/Frameworks/`; BridgeVM binaries link them dynamically (verify with
`otool -L`), so relinking/replacement by the user is possible as LGPL
requires. No LGPL code is statically linked into BridgeVM binaries.

## Verification

The position behind this document, and the checks that keep it true, are
explained in [`docs/licensing-and-attribution.md`](docs/licensing-and-attribution.md).

- Host linkage is audited with `scripts/verify-app-third-party-notices.sh`:
  every LGPL library must be a separate `.dylib` and have a dynamic consumer
  visible in `otool -L`; static archives are rejected.
- The standalone wimlib helper is audited with
  `scripts/verify-bundled-wimlib-runtime.sh`: its signed binary, version,
  system-only dynamic dependencies, GPL text, exact source archive and build
  receipts must agree.
- Locked Rust package names, versions, license expressions and registry sources
  are generated into `Contents/Resources/licenses/rust-dependencies.tsv`; their
  source license/notice files are bundled in `rust-license-texts.txt`.
- Guest payload contents are pinned by `scripts/win-assets/` and the driver
  package manifests (`viogpu3d-package-manifest.txt` in run evidence).
