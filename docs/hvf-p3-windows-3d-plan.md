# P3 — Windows guest 3D (`viogpu3d`) plan

Goal: GPU-accelerated 3D for the Windows 11 ARM64 guest on our from-scratch VMM,
reusing the host virtio-gpu 3D stack already exercised with Linux. Injection-ready
Windows ARM64 `viogpu3d` packages now exist locally, but none is currently a
render candidate. The VirGL full package carries five ARM64 Mesa DLLs and
`CopyFiles` entries, while its INF omits `UserModeDriverName`,
`OpenGLDriverName`, `OpenGLVersion`, `OpenGLFlags`, and
`InstalledDisplayDrivers`. The next piece is therefore a
regenerated/test-signed UMD-registered package, followed by live, boot-bound
proof that it installs, binds, and speaks the expected protocol: `venus` when it really
uses capset 4, or `virgl`/`virgl2` when it follows the older D3D10/GL path.

## The good news: our device is close to feature-compatible
The concrete VirGL package comes from the ARM64-capable `akre` branch of
`arehnman/kvm-guest-drivers-windows`, checked out at
`/Users/user/BridgeVM/viogpu3d-arehnman` (HEAD
`4c27e477e6560cea724d848b98149f03cb1f2083`). The original PR #943 snapshot is
still preserved at `/Users/user/BridgeVM/viogpu3d-pr943`. The package/source
report says:
- `protocol=virgl`
- `hwids=PCI\VEN_1AF4&DEV_1050`
- `arm64_configuration_present=true`
- `mesa_prefix_required=true`

That means PR #943 is the older VirGL/D3D10/GL path, not the Venus capset-4
path. BridgeVM's device model, trace gate, readiness gate, and installed HVF boot
path can now select either protocol identity. A PR #943 package should be booted
with `--gpu-trace-protocol virgl`, which selects
`BRIDGEVM_VIRTIO_GPU_3D_PROTOCOL=virgl` for the VM. A separate Venus WDDM
source/package would still be useful, but PR #943 is no longer blocked by host
VirGL renderer creation or by host runtime wiring: the macOS CGL probe can bring
up capset 1, and the installed boot path can select the CGL-backed VirGL runtime.

## Known device-side gaps (small, host-side, in our control)
1. **PCI bind id — prep done.** The signed 2D `viogpudo` path and PR #943
   `viogpu3d` source both bind `DEV_1050`; earlier `DEV_10F7` experiments are
   still supported for packages that use that alternate HWID. BridgeVM now keeps
   `DEV_1050` by default, exposes `DEV_10F7` for P3 by default through
   `BRIDGEVM_VIRTIO_GPU_3D_BIND_ID=1`, and lets installed boot runs override the
   exact PCI ID with `--virtio-gpu-device-id 1050|10f7`.
2. **Protocol identity.** `viogpu3d` packages are not all equivalent. The
   Vulkan path should use VENUS capset 4, while the D3D10/GL path may use
   VIRGL/VIRGL2 capset 1/2. BridgeVM now has a protocol-aware trace gate
   (`--protocol auto|venus|virgl`) and a package checker that refuses an
   unidentified package unless `VIOGPU3D_PROTOCOL=venus|virgl` is set after a
   source/package audit. Do not boot a package under a gate for the wrong
   protocol.

## The binary build wall is closed; the INF/UMD registration wall is not
`viogpu3d` is a WDDM kernel driver whose reproducible full build still requires:
1. **Mesa built on Windows** (`meson -Dgallium-drivers=virgl -Dgallium-d3d10umd`)
   producing the user-mode DLLs — a major build in its own right, and
   Mesa-on-Windows-**ARM64** is not a beaten path.
2. **Visual Studio 2022 + WDK (ARM64 target)** to build `viogpu3d.sys` — GUI,
   interactive, multi-GB installers.
3. Test-signing setup, then inject + boot.

That binary build was completed by CI. On 2026-07-10 the repository checker
found four injection-ready local ARM64 packages: a Venus KMD-only package at
`/Users/user/BridgeVM/venus-wddm-arm64`, two VirGL KMD-only candidates, and a
VirGL full package at
`/Users/user/BridgeVM/viogpu3d-prebuilt-candidates/arm64-ci/viogpu3d-full` with
five ARM64 Mesa DLLs. The full package records source
`akre@4c27e477e6560cea724d848b98149f03cb1f2083`, Mesa
`cb531c440ff34a9c6334859dda0848132be49ec3`, and build id `28945386687-8`.
All are self-signed test artifacts, not production-distributable drivers. The
three KMD-only packages have no UMD payload. The full package copies its five
DLLs but does not register the WDDM/OpenGL UMD names, so Windows has no INF
contract that selects those DLLs for rendering. The source
`viogpu3d_arm64.inx` does contain all five registrations; the package must be
regenerated from that source, then cataloged and test-signed as one immutable
unit. Merely editing the signed out-of-tree INF would invalidate its catalog.

The immediate wall is therefore a checker-accepted UMD-registered render
candidate. The following wall is live Windows evidence: certificate trust and
testsigning, `pnputil` install, a present `DEV_1050`/`DEV_10F7` device with
Status OK bound to the intended OEM INF, then a coherent capset/blob/context/
submit/fence trace tied to that same boot and a rendered workload.

## Reproducing or replacing the package
Two options remain for future rebuilds, in preference order:

**Option A — build inside our own Windows 11 ARM64 guest (we already boot one).**
Our `--daily` Windows desktop works (networked, 4-core, 6 GB). Steps (multi-
session, mostly interactive setup that a human or a scripted unattended install
drives):
1. In the guest: install VS 2022 Build Tools + WDK (ARM64), Python, meson, ninja,
   git; build Mesa (virgl + d3d10umd, static CRT) -> `MESA_PREFIX`.
2. Build PR #943 `viogpu/viogpu.sln` for `Win10 Release|ARM64` -> `viogpu3d.sys`,
   INF, CAT, and Mesa user-mode DLLs. The generated build kit below codifies this.
3. Inject the driver; the three-stage firstboot flow enables testsigning, trusts
   the certificate, installs the INF, reboots, and verifies the bound device,
   boot with `--virtio-gpu-3d`, `--virtio-gpu-device-id 1050`, and
   `--gpu-trace-protocol virgl`, then require the readiness + trace gates. Today
   the current package is no longer blocked by host VirGL support, runtime
   selection, kernel binary availability, or package HWID; it is blocked by its
   stripped fallback INF and the catalog/signature regeneration that an INF fix
   requires.
Could be partly automated with an unattended VS/WDK install image, but expect
interactive iteration.

**Option B — cross-build / external.** Provide a prebuilt `viogpu3d.sys` (ARM64,
test-signed) from any Windows ARM64 dev box; then this project injects + boots +
debugs — the parts our harness IS good at.

BridgeVM now generates the external build kit for this path:

```sh
scripts/prepare-hvf-windows-viogpu3d-build-kit.sh \
  --source-dir "$HOME/BridgeVM/viogpu3d-pr943" \
  --out-dir /tmp/bridgevm-viogpu3d-pr943-build-kit \
  --no-fetch
```

The generated `build-viogpu3d-arm64.ps1` is intended for an external Windows
ARM64 dev machine. The live source report generated on 2026-07-07 reports
`protocol=virgl`, `hwids=PCI\VEN_1AF4&DEV_1050`, `arm64_configuration_present=true`,
`bridgevm_default_installed_host_protocol=venus`,
`bridgevm_required_installed_host_protocol=virgl`,
and `boot_runtime_selector=--gpu-trace-protocol virgl`. Its historical
`boot_blocker=none` field predates the UMD-registration audit and must not be
read as render-readiness evidence.

## Where WE have the edge (the reason this is worth doing)
The community driver is stalled largely because guest-side crashes are
undebuggable in a black box. We are not a black box:
- Our VMM has a thin virtio-gpu command/fence/config recorder and renderer error
  logging, so a guest driver bug produces a reviewable trace. This is not yet a
  complete host-replay system.
- Serial KD over our proven UART (WinDbg serial) for kernel debugging.
- Linux and synthetic host/device-model evidence reduce the host-side unknowns,
  but they do not prove the Windows VirGL command stream or presentation path.

## Bring-up ladder (current)
1. `viogpu3d.sys` loads clean (Device Manager, no code 10/43). Keep `viogpudo`
   as the display path so the desktop never regresses while 3D stabilizes.
2. Host trace gate passes with the package's identified protocol:
   `--protocol venus` for capset 4 or `--protocol virgl` for capset 1/2.
3. Guest protocol-specific user-mode test passes (`vulkaninfo`/`vkcube` for
   venus; D3D10/GL smoke for virgl).
4. DXVK d3d11 → a real DX11 title (the Parallels-parity flag).
5. vkd3d-proton d3d12 (beyond Parallels).

## Status
- Driver source located + branch identified; device feature-compatibility
  confirmed by reading `viogpu_adapter.cpp`.
- Host venus stack proven (Linux, P2 + GPU-execution + 117-136 GB/s).
- P3 host-side observability has started: `BRIDGEVM_VIRTIO_GPU_TRACE_JSONL=/path/to/trace.jsonl`
  now enables an env-gated JSONL recorder in the HVF virtio-gpu device. It
  records device shape, feature negotiation, queue notify state, command
  request/response names, capset/blob/context/submit fields, and fence
  create/complete/deliver events. This is intentionally a thin bring-up
  recorder, not a full replay system; the immediate gate is Windows
  `viogpu3d` bind + first capset/blob/context/fence trace.
- The trace now has a CLI gate report:
  `bridgevm hvf virtio-gpu-trace-report --trace /path/to/trace.jsonl --protocol auto --require-p3-gate`.
  It reports each P3 bring-up condition separately and exits non-zero if the
  trace is missing feature acceptance, queue notify, a coherent `venus` or
  `virgl` capset identity, matching `context_init`, blob creation, a non-empty
  `SUBMIT_3D`, or a backend-parked fenced command with fence delivery.
- Before real Windows, the synthetic host preflight can exercise BridgeVM's
  device-model host-visible blob map/unmap, non-empty submit, and renderer-fence
  callback path without QEMU, Apple VZ, or guest execution:
  `bridgevm hvf virtio-gpu-3d-host-preflight`. It defaults to the current
  `venus` contract and also accepts `--protocol virgl` to prove the synthetic
  capset-1/context-init device-model path. This is still not a Windows
  end-to-end pass; it is a host/device-model preflight.
- The live host renderer probe now makes the PR #943 host backend explicit:
  `scripts/run-venus-host-probe.sh` still passes with `host_renderer_venus=AVAILABLE`,
  while `scripts/run-virgl-host-probe.sh` records
  `host_renderer_virgl=AVAILABLE` on the current macOS build using
  CGL/OpenGL callbacks (`gl_context_callbacks=cgl-opengl`,
  `VIRGL_CAPSET_OK ver=1 size=308`). The installed HVF boot path can select this
  VirGL runtime with `BRIDGEVM_VIRTIO_GPU_3D_PROTOCOL=virgl`, and the wrapper
  sets that when `--gpu-trace-protocol virgl` is requested.
- The no-VM readiness check wires host and package evidence together:
  `scripts/check-hvf-windows-p3-gpu-readiness.sh --driver-dir /path/to/test-signed/viogpu3d --pci-device-id 1050 --require-driver-package`.
  It runs the synthetic host preflight, runs the package checker, and reports
  package-protocol device-model evidence separately from the current host
  backend. It also invokes the package checker's stricter
  `--require-render-candidate` contract: KMD-only packages and DLL-bearing
  packages without complete `UserModeDriverName`, `OpenGLDriverName`,
  `OpenGLVersion`, `OpenGLFlags`, and `InstalledDisplayDrivers` INF registration
  fail before boot. The registered DLL names must also resolve through active
  `CopyFiles` entries into DirID 11. The default
  installed runtime remains `venus`, so a valid `virgl` package still fails fast
  unless the VirGL runtime is selected. With
  `BRIDGEVM_VIRTIO_GPU_3D_PROTOCOL=virgl` or installed boot
  `--gpu-trace-protocol virgl`, the same package can report
  `host_backend_virgl_runtime=WIRED`. Add `--probe-host-renderer` when live host
  renderer evidence is needed; on the current macOS build that records
  `host_renderer_virgl=AVAILABLE` and
  `host_renderer_virgl_gl_context_callbacks=cgl-opengl`.
- The artifact inventory scanner removes the repeated manual search step:
  `scripts/find-hvf-windows-viogpu3d-packages.sh --root "$HOME/BridgeVM" --out-dir /tmp/bridgevm-viogpu3d-inventory --require-found`.
  It discovers candidate directories from viogpu3d `DEV_1050`/`DEV_10F7` INFs or
  `viogpu3d` SYS filenames, runs the package checker, writes per-candidate
  manifests, and reports `ready_count` for injection-ready packages separately
  from `render_candidate_count`; `--require-render-candidate` enforces the
  latter without hiding KMD-only inventory. The earlier
  2026-07-07 source-only scan (`ready_count=0`) is superseded: the 2026-07-10
  checker passes the Venus KMD package and three VirGL packages, including the
  full ARM64 SYS/INF/CAT + five-DLL package described above. The current
  render-candidate count is nevertheless zero because that full INF lacks all
  five UMD/OpenGL registrations.
- The injector/boot harness now has a P3 path for those real driver packages:

  ```sh
  scripts/find-hvf-windows-viogpu3d-packages.sh \
    --root "$HOME/BridgeVM" \
    --out-dir /tmp/bridgevm-viogpu3d-inventory \
    --require-render-candidate

  scripts/check-hvf-windows-viogpu3d-package.sh \
    --manifest /tmp/bridgevm-p3-gpu/viogpu3d-package-manifest.txt \
    --pci-device-id 1050 \
    --require-render-candidate \
    /path/to/test-signed/viogpu3d

  scripts/check-hvf-windows-p3-gpu-readiness.sh \
    --driver-dir /path/to/test-signed/viogpu3d \
    --manifest /tmp/bridgevm-p3-gpu/viogpu3d-package-manifest.txt \
    --pci-device-id 1050 \
    --require-driver-package

  VIOGPU3D_DIR=/path/to/test-signed/viogpu3d \
    scripts/build-hvf-windows-viogpu3d-injector.sh

  scripts/run-hvf-windows-installed-boot.sh \
    --target /path/to/windows-target.raw \
    --vars /path/to/vars.fd \
    --placeholder-nsid1 "$HOME/BridgeVM/win-viogpu3d-injector.raw" \
    --evidence-dir /tmp/bridgevm-p3-gpu \
    --virtio-gpu-3d \
    --virtio-gpu-device-id 1050 \
    --gpu-trace /tmp/bridgevm-p3-gpu/virtio-gpu.jsonl \
    --gpu-trace-protocol virgl \
    --viogpu3d-dir /path/to/test-signed/viogpu3d \
    --require-viogpu3d-readiness \
    --require-gpu-trace-gate

  bridgevm hvf virtio-gpu-trace-report \
    --trace /tmp/bridgevm-p3-gpu/virtio-gpu.jsonl \
    --protocol auto \
    --require-p3-gate
  ```

  The checker/wrapper validates audited `PCI\VEN_1AF4&DEV_1050` or
  `PCI\VEN_1AF4&DEV_10F7` driver package shape, requires a `.cat` catalog,
  rejects non-ARM64 PE `.sys`/`.dll` binaries,
  requires a `venus`/`virgl` protocol identification, and can write a package
  manifest with source metadata, file sizes, SHA-256 hashes, and PE machine
  fields. Its default `package_capability` preserves KMD-only packages as
  injection-ready inventory; `--require-render-candidate` requires a VirGL UMD
  payload plus complete active INF registration. Externally built packages can carry
  `bridgevm-package-provenance.env`; the checker auto-loads it to recover the
  PR source commit, build id, signing note, protocol, and expected HWID before
  package validation. The injector stages `viogpu3d` under `\drivers\viogpu3d`
  and plants a fail-closed firstboot state machine. Stage 1 enables testsigning
  and trusts the certificate, stage 2 installs and rescans the driver, and stage
  3 requires Status OK plus the intended bound OEM INF. The boot
  harness builds the probe with the `venus` feature,
  exposes `DEV_10F7` by default or the requested `--virtio-gpu-device-id`, writes
  the GPU JSONL trace, and stores
  `virtio-gpu-trace-report.txt` plus `virtio-gpu-trace-gate.txt` in the evidence
  directory when `--require-gpu-trace-gate` is set. When `--viogpu3d-dir` is
  provided, it runs the same readiness check before building/running the VM and
  writes `p3-gpu-readiness.txt` plus `viogpu3d-package-manifest.txt`; with
  `--require-viogpu3d-readiness`, a missing, KMD-only, UMD-unregistered, or
  protocol-incompatible package stops the boot before guest execution. The installed boot wrapper maps
  `--gpu-trace-protocol virgl` to `BRIDGEVM_VIRTIO_GPU_3D_PROTOCOL=virgl`.
  `PROBE_HOST_RENDERER=1` can be used with the readiness script to add the live
  Venus/VirGL renderer probe result to the evidence.
- BLOCKED first on regenerating the VirGL full package from an INF with active
  UMD registrations, then rebuilding its catalog and test signature. Once the
  checker reports `render_candidate=true`, the next concrete blocker is a
  verifier-accepted live run proving test-sign trust, PnP bind/Status OK and the
  intended OEM INF, followed by a trace from that same run that passes the
  protocol-coherent feature/capset/blob/context/submit/fence gate. Production
  signing, licensing, stability, and workload/render proof remain later gates.

_Updated 2026-07-10. See [[bridgevm-hvf-engine-status]] and
docs/hvf-3d-engine-plan.md._
