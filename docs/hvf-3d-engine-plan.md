# BridgeVM 3D Engine — Deep Implementation Plan (Parallels-class 3D)

Goal: real GPU-accelerated 3D for guests of our from-scratch HVF VMM —
Parallels-class for Windows 11 ARM64, and beyond-Parallels where the open stack
lets us leapfrog (guest Vulkan, and eventually D3D12 via vkd3d-proton, which
Parallels does NOT have).

Strategy in one line: **we build the hard glue nobody ships (the 3D-capable
virtio-gpu device model on HVF, and the Windows ARM64 guest driver maturity),
and we reuse the hard-but-solved translation layers (Venus, virglrenderer,
MoltenVK/KosmicKrisp, DXVK, vkd3d-proton) instead of re-fighting Parallels'
15-year D3D→Metal war.**

## 1. The stack (bottom = host silicon)

```
Windows guest app
  ├─ D3D9/10/11  → DXVK (open, Valve-hardened)      ┐
  ├─ D3D12       → vkd3d-proton (open)               ├─→ Vulkan (guest)
  └─ Vulkan      → directly                          ┘
Guest Vulkan ICD = Mesa VENUS driver (Windows build; thin Vulkan serializer)
  ↓ Venus protocol (Vulkan 1.3+ command serialization, ~zero translation)
Guest WDDM KMD (virtio-gpu 3D: contexts, blob resources, fences, scanout)
  ↓ virtqueues on OUR virtio-gpu PCI device (already shipping 2D @00:05.0)
OUR VMM device model (Rust): SUBMIT_3D, RESOURCE_CREATE_BLOB, MAP_BLOB
  (host GPU memory mapped into a guest PCI BAR), fences, capsets
  ↓ FFI
libvirglrenderer (-Dvenus, macOS build; freedesktop upstream)
  ↓ host Vulkan
KosmicKrisp (Mesa Vulkan-on-Metal, upstreamed 2025, conformance-targeting)
  — fallback: MoltenVK (portability subset; misses geometry shaders etc.)
  ↓
Apple Metal → M-series GPU
```

Same host stack serves **Linux guests with zero extra work** (Mesa venus is
in-tree on Linux) — that is our de-risking order, not an afterthought.

## 2. Build vs reuse (the "don't rebuild everything" discipline)

| Layer | Decision | Why |
|---|---|---|
| D3D→Vulkan translation | REUSE DXVK / vkd3d-proton | Valve-hardened, years of game fixes; rebuilding = Parallels' moat war |
| Guest Vulkan driver | REUSE Mesa Venus ICD | Thin serializer, stable since 2023 (VK 1.3+) |
| Venus decode + host render | REUSE libvirglrenderer (venus) | Proven on macOS by libkrun/krunkit |
| Vulkan-on-Metal | REUSE KosmicKrisp (fallback MoltenVK) | KosmicKrisp targets full conformance = what DXVK needs |
| **virtio-gpu 3D device model on HVF** | **BUILD (us)** | Nobody ships this for a from-scratch macOS VMM; our core competence |
| **HVF host-GPU-memory mapping (blob/HOSTMEM BAR)** | **BUILD (us)** | hv_vm_map of Metal shared memory into guest PA — novel, hard, ours |
| **Windows ARM64 WDDM KMD + ICD maturity** | **CO-BUILD (fork + harden)** | anonymix007's venus WDDM driver exists but is experimental, unmerged, unproven on ARM64 — we become its ARM64 proving ground |
| Shader compile (DXBC/DXIL→SPIRV→MSL) | REUSE (DXVK's compiler + KosmicKrisp/MoltenVK) | An entire compiler stack we skip |
| GL/virgl path | SKIP entirely | Venus-only; GL apps ride Zink-on-Vulkan if ever needed |

## 3. Phases (each independently valuable; strict gates)

### Phase 0 — Host renderer foundation (no guest, no VMM changes) [2-4w]
- Build libvirglrenderer with `-Dvenus=true` on macOS (libkrun precedent);
  bindgen Rust FFI. Build/obtain KosmicKrisp; MoltenVK fallback.
- Standalone harness: feed a captured Venus command stream (from a Linux
  vkcube run under crosvm/libkrun) into virglrenderer→KosmicKrisp on macOS;
  render offscreen; checksum frames.
- Decide execution architecture: dedicated renderer THREAD in the probe process
  (Metal + ObjC constraints), lock-free submit/completion rings to the vCPU
  loop; completions flushed via existing pending_msix infra.
- **Gate**: harness renders correct frames. **Kill**: virglrenderer/KosmicKrisp
  fundamentally broken on macOS-arm64 → re-scope to MoltenVK subset or stop.
- **✅ P0 RESULT (2026-07-05, gate PASSED)**: upstream virglrenderer (2a173ee)
  builds CLEAN on macOS arm64 with `-Dvenus=true` (the libkrun macOS work is
  upstream; Metal/Foundation detected natively). Architecture: venus lives in
  the separate `virgl_render_server` process (crash isolation, crosvm model),
  the lib is a proxy, the server dlopens MoltenVK. Our probe
  (`tools/venus-host-probe`, gate `scripts/run-venus-host-probe.sh`) got
  `VENUS_CAPSET_OK`: capset id 4, 160 bytes, wire-format v1, venus protocol
  spec v4, supports_blob_id_0, multiple timelines; context create/destroy OK.
  Init flags 0x3c2 (VENUS|NO_VIRGL|RENDER_SERVER|THREAD_SYNC|ASYNC_FENCE_CB),
  callbacks v4 (write_fence + write_context_fence). MoltenVK first;
  KosmicKrisp evaluation deferred to P2 benchmarking.

### Phase 1 — Device model: 3D virtio-gpu (the core VMM build) [3-6w]
Extend `src/virtio_gpu.rs` (2D stays intact and default):
- Feature bits: RESOURCE_BLOB, CONTEXT_INIT (+VIRGL where guests require it);
  Venus capset via GET_CAPSET_INFO/GET_CAPSET.
- Commands: CTX_CREATE/DESTROY/ATTACH/DETACH_RESOURCE, SUBMIT_3D,
  TRANSFER_TO/FROM_HOST_3D, RESOURCE_CREATE_BLOB, SET_SCANOUT_BLOB,
  RESOURCE_MAP_BLOB/UNMAP_BLOB.
- **HOSTMEM shared-memory region**: new 64-bit prefetchable BAR (1-8 GiB
  window, VIRTIO_GPU_SHM_ID_HOST_VISIBLE). MAP_BLOB → hv_vm_map the
  virglrenderer-exported host pages (MTLBuffer storageModeShared contents;
  validate page alignment) into the BAR at the requested offset; UNMAP_BLOB →
  hv_vm_unmap. This is THE novel HVF engineering — get it right early
  (alignment, lifetime, unmap-while-mapped safety, teardown on reset).
- Fences: virglrenderer write_fence callback → completion ring → used-ring +
  MSI-X. Env-gate everything: `BRIDGEVM_VIRTIO_GPU_3D=1`.
- **Gate**: unit tests (blob lifecycle, map/unmap, fence ordering, ctx
  teardown) + capset visible to a Linux guest. 2D-only mode byte-identical.

### Phase 2 — Linux guest proof (de-risk the whole host side) [1-3w]
- Boot our proven Linux guest (Fast-Mode recipe) with Mesa venus; run
  vkcube/vkgears/vkmark on OUR VMM. Find 80% of host bugs with 20% of the pain
  (no driver signing, in-tree guest driver, full dmesg).
- Benchmark venus overhead vs host-native Metal; fence latency; blob churn.
- **Gate**: stable vkcube + vkmark run >10min, no leak, no fence hang.
  **This alone ships "real 3D VM on a from-scratch engine" — QEMU/UTM-class-beating for Linux guests.**

### Phase 3 — Windows ARM64 guest driver (the moat crossing) [3-6mo, parallel-track]
The honest hard part. Base = fork `anonymix007/kvm-guest-drivers-windows-venus`
(WDDM KMD + Mesa venus ICD for Windows) and become its ARM64 proving ground.
- 3a. Build the KMD with the ARM64 WDK; boot it in test-signing mode
  (`bcdedit /set testsigning on`, set offline by our injector). Merge/replace
  the display path with our PROVEN viogpudo-style scanout so display never
  regresses while 3D is unstable.
- 3b. Debugging story (prerequisite, do FIRST): serial KD over our UART
  (WinDbg serial), plus our unique superpower — the VMM traces EVERY
  virtio-gpu command/fence/BAR access. Add a `BRIDGEVM_VIRTIO_GPU_TRACE=1`
  command-stream recorder + replayer so guest-driver bugs become
  host-replayable artifacts. (Synergy: same serial-KD unlocks the old
  DRIVER_PNP_WATCHDOG investigation.)
- 3c. Mesa venus ICD Windows ARM64 build; **ship BOTH arm64 and x64 ICDs**
  (x64 apps under Prism emulation load x64 DLLs in-process; kernel boundary is
  handled by emulation). Same dual-arch for DXVK later.
- 3d. Presentation/WSI: implement VK_KHR_swapchain in the ICD → KMD present →
  blit into scanout resource + RESOURCE_FLUSH (reuse our 2D present path).
- Ladder: driver loads clean → vkcube.exe (arm64) → vkcube x64-emulated →
  DXVK d3d11 triangle → DXVK real app (e.g. an older DX11 game) →
  vkd3d-proton hello-triangle (the beyond-Parallels flag).
- **Gate per rung; kill/park criteria**: if the KMD can't reach stable
  vkcube in ~8 focused weeks, PARK Windows 3D (keep Linux 3D + 2D display
  shipping) and upstream our fixes — the fork keeps compounding via community.

### Phase 4 — Parallels-class polish [ongoing]
- Perf: fence batching, per-context rings, HOSTMEM sizing (libkrun hit an
  8 GiB ceiling — plan BAR sizing policy), damage-rect present, SMP interrupt
  spread (our MSI-X 9-vector work generalizes).
- Compat: DXVK per-app deployment first (games you choose); investigate
  system-wide D3D11 later — a WDDM D3D UMD shim wrapping DXVK is the one
  genuinely moat-ish piece; defer until the Vulkan floor is rock-solid.
- Distribution: test-signing for dev; Microsoft attestation signing (EV cert)
  when productizing. Signing is money+process, not research risk.
- Benchmarks vs Parallels 26: GFXBench/3DMark-class + 3 real DX11 games.
  Target: within 2x of Parallels DX11 initially; DX12-via-vkd3d as the
  differentiator they can't answer.

## 4. Top risks (ranked) & mitigations
1. **Windows KMD stability on ARM64** (the community driver crashes there
   today). Mitigation: serial KD + command-stream replayer + our full-VMM
   tracing; display path stays viogpudo-proven; strict rung gates.
2. **KosmicKrisp maturity** for DXVK-level feature demands. Mitigation:
   track Mesa releases; MoltenVK fallback covers Vulkan-only apps (no
   geometry-shader-dependent DXVK paths); Phase 0 harness measures this on
   day one, before we invest guest-side.
3. **HVF map/unmap semantics** for host GPU memory (alignment, TLB, unmap
   races). Mitigation: Phase 1 unit-tests it in isolation; conservative
   map-once/cache policy first; stress harness.
4. **Venus protocol drift** (Mesa guest vs virglrenderer host versions).
   Mitigation: pin a tested pair; capset gates the contract explicitly.
5. **Scope creep toward "all apps day one."** Mitigation: the ladder is the
   product — Linux 3D ships first, Windows Vulkan second, per-app DXVK third,
   system-wide D3D last (or never, and that's still a good product).

## 5. Effort & sequencing reality
- Phases 0-2 (host + Linux 3D): ~2-3 months of our normal cadence — high
  confidence, all precedented (libkrun), our infra advantages apply.
- Phase 3 (Windows): 3-6 months specialist-track, medium confidence — but
  every sub-rung produces durable value (ARM64 fixes upstreamed, KD infra,
  trace/replay tooling), and it runs in parallel with everything else.
- We do NOT need Parallels' headcount because we refuse their architecture:
  they translate APIs (huge surface, forever); we transport Vulkan (thin,
  stable protocol) and let the open ecosystem do translation at both ends.

## 6. Success criteria
- **S1 (Linux 3D)**: vkmark stable on our VMM at >50% of host-native Metal
  throughput. Beats: QEMU/UTM Linux-GL-only story.
- **S2 (Windows Vulkan)**: vkcube + a Vulkan game runs on Windows 11 ARM64
  guest. Beats: everyone except Parallels (who have NO guest Vulkan at all —
  this is already differentiation).
- **S3 (Windows D3D11 via DXVK)**: 3 real DX11 titles playable. Parity-class
  with Parallels for those titles.
- **S4 (D3D12 via vkd3d-proton)**: any DX12 title running = capability
  Parallels has never shipped.

## 7. Licensing (clean-by-construction for a commercial, closed VMM)

The stack was chosen so the proprietary core stays proprietary and every reused
layer is permissive. Rules and per-component status:

| Component | License | Our use | Verdict |
|---|---|---|---|
| Our VMM / device models | ours | proprietary core | — |
| Mesa (Venus ICD, KosmicKrisp) | MIT | guest ICD (redistribute) / host Vulkan (link) | ✅ attribution only |
| virglrenderer (venus) | MIT | host lib, FFI-linked into our VMM | ✅ attribution only |
| MoltenVK (fallback) | Apache-2.0 | host lib | ✅ attribution + NOTICE; patent grant is a plus |
| DXVK (D3D9/10/11→VK) | zlib | guest DLLs (redistribute) | ✅ no obligations beyond no-misrepresentation |
| virtio-win drivers (viogpudo, netkvm) | BSD-3-Clause (relicensed by Red Hat for redistribution) | guest drivers | ✅ attribution; may also build+sign our own from source |
| venus WDDM driver fork (anonymix007 → us) | expect BSD-3 + MIT (inherits virtio-win + Mesa) | guest KMD/ICD | ✅ expected; **verify exact terms at Phase-3 entry**; keep OUR fork open-source (strategically desirable anyway — upstream collaboration is how the ARM64 driver matures) |
| vkd3d-proton (D3D12→VK) | **LGPL-2.1** | guest d3d12 DLLs | ⚠️ OK if shipped as SEPARATE, user-replaceable DLLs + license text + source offer (standard practice). If we ever refuse LGPL entirely, we defer D3D12 — Vulkan/DX11 story is unaffected |
| EDK2 firmware (vendored) | BSD-2-Clause-Patent | redistributed blob | ✅ keep license text |
| wimlib, hdiutil, WDK | (LGPLv3 / Apple / MS EULA) | host/build TOOLS only, never linked | ✅ tool use imposes nothing on our binaries |
| Windows 11 itself | Microsoft EULA | user-provided ISO/license | ✅ we never redistribute Windows bits |

Hard rules (already our practice, now explicit):
1. **Never copy GPL code into the engine or drivers** — no QEMU source, no Wine
   source, no Parallels guest-tools source (their prl_* Linux drivers are
   GPL/proprietary — we copied only the architectural *pattern*, never code).
   Everything we implement is spec-driven (virtio spec, PCIe, NVMe, WDDM DDI),
   which is exactly how the engine was built so far.
2. **Host-linked libraries must be MIT/Apache/BSD only** (virglrenderer, Mesa,
   MoltenVK qualify). Copyleft components may only ever be separate processes
   or guest-side artifacts, never linked into the VMM.
3. **Guest-side open components stay cleanly separated** from any proprietary
   guest agent we later write; ship a third-party-licenses file with exact
   texts and per-component source links.
4. **Apple GPTK / D3DMetal is license-banned** (evaluation-only, no
   redistribution) — irrelevant to our chosen stack; noting it so nobody
   "helpfully" adds it later.
5. Phase-3 entry checklist gains one item: read the exact LICENSE files of the
   venus-WDDM fork tree and each virtio-win driver we redistribute, and record
   them in the third-party manifest.

_Grounding: 2026-07-05 research pass (Parallels DX11.1-only/no-DX12/no-Vulkan;
Venus stable VK1.3+ since 2023; KosmicKrisp upstreamed to Mesa late 2025;
libkrun/krunkit venus→MoltenVK precedent on Apple Silicon; viogpu3d stalled &
ARM64-crashing; anonymix007 venus WDDM driver experimental). See
[[hvf-graphics-integration-gap-plan]] for the 2D/integration roadmap this
builds on — our virtio-gpu 2D + viogpudo 1080p desktop is already live._
