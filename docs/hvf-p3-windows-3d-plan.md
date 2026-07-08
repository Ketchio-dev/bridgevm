# P3 — Windows guest 3D (`viogpu3d`) plan

Goal: GPU-accelerated 3D for the Windows 11 ARM64 guest on our from-scratch VMM,
reusing the host virtio-gpu 3D stack already proven with Linux (compute + image,
117-136 GB/s). The remaining piece is a Windows ARM64 `viogpu3d` driver package
whose actual protocol path is identified before boot: `venus` when it really
uses capset 4, or `virgl`/`virgl2` when it follows the older D3D10/GL path.

## The good news: our device is close to feature-compatible
The concrete source we can build today is virtio-win PR #943, mirrored in
`max8rr8/kvm-guest-drivers-windows` branch `viogpu3d`. We checked it out at
`/Users/user/BridgeVM/viogpu3d-pr943` (HEAD
`9ed3aab11fb46e55dc835ff008008623b290a6cf`). Its source report says:
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

## The real wall: the BUILD (physical constraint, honest)
`viogpu3d` is a WDDM kernel driver whose full build (per its `BUILDING.md`)
requires, INSIDE a Windows dev VM:
1. **Mesa built on Windows** (`meson -Dgallium-drivers=virgl -Dgallium-d3d10umd`)
   producing the user-mode DLLs — a major build in its own right, and
   Mesa-on-Windows-**ARM64** is not a beaten path.
2. **Visual Studio 2022 + WDK (ARM64 target)** to build `viogpu3d.sys` — GUI,
   interactive, multi-GB installers.
3. Test-signing set up, then inject + boot.

This cannot be driven by our headless boot-probe harness. It needs an
interactive Windows ARM64 developer environment. That is the genuine
physical limit for a fully-autonomous session.

## Realistic path across the wall
Two options, in preference order:

**Option A — build inside our own Windows 11 ARM64 guest (we already boot one).**
Our `--daily` Windows desktop works (networked, 4-core, 6 GB). Steps (multi-
session, mostly interactive setup that a human or a scripted unattended install
drives):
1. In the guest: install VS 2022 Build Tools + WDK (ARM64), Python, meson, ninja,
   git; build Mesa (virgl + d3d10umd, static CRT) -> `MESA_PREFIX`.
2. Build PR #943 `viogpu/viogpu.sln` for `Win10 Release|ARM64` -> `viogpu3d.sys`,
   INF, CAT, and Mesa user-mode DLLs. The generated build kit below codifies this.
3. `bcdedit /set testsigning on` (offline via our injector), inject the driver,
   boot with `--virtio-gpu-3d`, `--virtio-gpu-device-id 1050`, and
   `--gpu-trace-protocol virgl`, then require the readiness + trace gates. Today
   PR #943 remains blocked by the missing Windows ARM64 package, not by host
   VirGL support, runtime selection, or package HWID.
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
`boot_runtime_selector=--gpu-trace-protocol virgl`, and `boot_blocker=none`.

## Where WE have the edge (the reason this is worth doing)
The community driver is stalled largely because guest-side crashes are
undebuggable in a black box. We are not a black box:
- Our VMM traces every virtio-gpu command / fence / BAR access, and we added
  venus CS-error logging in virglrenderer — a guest driver bug becomes a
  host-replayable artifact.
- Serial KD over our proven UART (WinDbg serial) for kernel debugging.
- The host venus stack is FULLY VERIFIED (Linux), so any failure isolates to the
  guest driver — one unknown, not two.

## Bring-up ladder (once a driver binary exists)
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
  backend. The default installed runtime remains `venus`, so a `virgl` package
  still fails fast unless the VirGL runtime is selected. With
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
  manifests, and reports how many packages are injection-ready. After checking
  out PR #943 source, a live scan on 2026-07-07 finds that source directory as
  one rejected candidate (`candidate_count=1`, `ready_count=0`,
  `candidate_reject_reason=FAIL: no .inf found ...`); there is still no
  injection-ready package.
- The injector/boot harness now has a P3 path for the first real driver package:

  ```sh
  scripts/find-hvf-windows-viogpu3d-packages.sh \
    --root "$HOME/BridgeVM" \
    --out-dir /tmp/bridgevm-viogpu3d-inventory \
    --require-found

  scripts/check-hvf-windows-viogpu3d-package.sh \
    --manifest /tmp/bridgevm-p3-gpu/viogpu3d-package-manifest.txt \
    --pci-device-id 1050 \
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
  fields. Externally built packages can carry
  `bridgevm-package-provenance.env`; the checker auto-loads it to recover the
  PR source commit, build id, signing note, protocol, and expected HWID before
  package validation. The injector stages `viogpu3d` under `\drivers\viogpu3d`
  and plants the WinPE marker that enables offline BCD test-signing. The boot
  harness builds the probe with the `venus` feature,
  exposes `DEV_10F7` by default or the requested `--virtio-gpu-device-id`, writes
  the GPU JSONL trace, and stores
  `virtio-gpu-trace-report.txt` plus `virtio-gpu-trace-gate.txt` in the evidence
  directory when `--require-gpu-trace-gate` is set. When `--viogpu3d-dir` is
  provided, it runs the same readiness check before building/running the VM and
  writes `p3-gpu-readiness.txt` plus `viogpu3d-package-manifest.txt`; with
  `--require-viogpu3d-readiness`, a missing or incompatible package stops the
  boot before guest execution. The installed boot wrapper maps
  `--gpu-trace-protocol virgl` to `BRIDGEVM_VIRTIO_GPU_3D_PROTOCOL=virgl`.
  `PROBE_HOST_RENDERER=1` can be used with the readiness script to add the live
  Venus/VirGL renderer probe result to the evidence.
- BLOCKED on one concrete remaining item before real Windows 3D bring-up: a
  Windows ARM64 build environment (VS+WDK+Mesa) or external Windows ARM64 dev box
  to produce the PR #943 ARM64 package. The HWID gate, package checker,
  build-kit generator, injector path, host VirGL renderer probe, runtime
  selector, and trace gate are ready.

_2026-07-07. See [[bridgevm-hvf-engine-status]] and
docs/hvf-3d-engine-plan.md._
