# bridgevm-hvf `lib.rs` structural-debt extraction plan

Document status: **Active plan**
Adopted: **2026-07-22**

This plan governs the behavior-preserving decomposition of
`crates/bridgevm-hvf/src/lib.rs` (34,750 lines, 296 `unsafe` sites at baseline).
It was produced from two independent analyses that were run in parallel and
**converged on the same thesis**: `gpt-5.6-sol` (medium) as the primary planner
per the subagent model policy, corroborated by a 12-reader Opus mapping
workflow. Nothing here changes runtime behavior; it defines the sequence of
small, independently reviewable extraction PRs.

## 1. What `lib.rs` actually is

The real device implementations already live in focused sibling modules
(`nvme`, `xhci`, `pcie`, `platform_virt`, `tpm_tis`, `tpm_ppi`, `ramfb`,
`virtio_*`, `pl011`, `pl031`, …). The root `lib.rs` is a **stratified legacy
probe monolith** with four layers:

| Layer | Lines (approx) | `unsafe` | Contents |
| --- | --- | --- | --- |
| L1 host-side | 1–14,148 | **0** | ~57 `Hvf*`/`WindowsArm*` probe-result structs + `render_text()` evidence formatters; synthetic MMIO harness (`MmioBus`, PL011/PL031/GICv3/virtio-mmio-block **diagnostic** models); virtq/guest-memory primitives; storage backends; FDT builder/parser; GPT/UEFI-FV byte tooling; AArch64 ESR/ISS decoders; `LowVectorPostRepair*` telemetry; machine-plan / no-QEMU gate types |
| L2 Apple platform | 14,149–26,395 | **all 296** | the `#[cfg(macos+aarch64)] mod platform` block: the single `#[link(name="Hypervisor")] extern "C"` FFI surface and every live `hv_vm_*`/`hv_vcpu_*` probe backend plus raw guest-memory pointer IO |
| L3 fallback | 26,397–28,039 | 0 | the non-Apple-Silicon `mod platform` returning fully-defaulted probe structs |
| L4 tests | 28,042–34,749 | — | one ~6,700-line `#[cfg(test)] mod tests` reaching everything via `use super::*`, plus a dozen small scattered inline test modules |

**Extraction thesis:** the file is already cleanly stratified by
responsibility. Lift the safe L1 layer out first as independently-testable
leaf / device / telemetry / probe modules (creating stable import targets),
**then** split the single `unsafe` cfg-gated platform block by responsibility
behind one FFI module — so all 296 `unsafe` sites end up isolated by concern
under a `platform/` subtree, never hidden or deleted. Public API is preserved
by explicit `pub use` re-export shims at the crate root; inline tests travel
with the code they cover.

## 2. Baseline and budget mechanism

Baseline (seeded in `scripts/refactor-budgets.tsv`, enforced by
`scripts/check-refactor-budgets.sh`):

| File | LOC ceiling | `unsafe` ceiling |
| --- | --- | --- |
| `crates/bridgevm-hvf/src/lib.rs` | 34,750 | 296 |
| `crates/bridgevm-api/src/lib.rs` | 13,868 | 3 |
| `crates/bridgevm-daemon/src/main.rs` | 5,969 | 4 |
| `crates/bridgevm-storage/src/lib.rs` | 5,321 | 0 |

The repository has no hosted CI, so the ratchet script is the non-increase
gate. As a file shrinks, add its new sibling modules to the budgets file and
lower the shrunk file's ceiling — but **only for a genuine aggregate reduction**,
never for a pure move (see §5).

## 3. Per-packet gate (every PR must satisfy)

- Preserve all existing crate-root public paths with private modules + explicit
  `pub use`; do not make extracted implementation modules public just to
  simplify visibility, and avoid wildcard re-exports (they can expand the public
  API).
- Preserve the two existing `mod platform` cfg predicates **verbatim**; do not
  "simplify" platform detection while extracting.
- Move code before redesigning it. Visibility changes are the minimum
  `pub(crate)`/`pub(super)` required.
- Move inline tests into the module owning the code they primarily exercise;
  shared fixtures go into a private `#[cfg(test)] mod test_support`, never a
  standalone dumping-ground test file. Do **not** relocate the test module in
  bulk to shrink the production count.
- Compare every `render_text()` / blocker / evidence string **byte-for-byte**
  (ordering, punctuation, numeric formatting, status names).
- Preserve opt-in checks before live HVF operations and all
  unsupported-host / failure behavior.
- Record every moved `unsafe` site in the PR as `old file:line -> new file:line`;
  body and count unchanged (safety rewrites are a separate later series).
- Run the supported-target build, the unsupported-target build (`--target` a
  non-macOS triple or a cfg check), `cargo test -p bridgevm-hvf --all-targets`,
  `cargo fmt --all -- --check`, `scripts/check-refactor-budgets.sh`, and
  `git diff --check` after each packet.

## 4. Target module layout

```text
crates/bridgevm-hvf/src/
├── lib.rs                     # facade: device-mod decls + private mod decls + explicit re-exports
├── support.rs                 # HvfSupport, detect_hvf_support, HV return/exit names, WindowsArmVmmGateStatus
├── no_qemu_plan.rs            # WindowsArmVmmGate, WindowsArmNoQemuPlan + builders/gates
├── machine_plan.rs           # HvfMachinePlan* family, device/memory rosters, render_text
├── probes/                    # public diagnostic probe-result structs + safe fn wrappers -> platform::
│   ├── mod.rs · host_capabilities.rs · vm_create.rs · vcpu_create.rs · vcpu_run.rs
│   ├── interrupt_timer.rs · vtimer_exit.rs · memory_map.rs · guest_entry.rs · guest_exit_loop.rs
│   ├── mmio_read_exit.rs · mmio_read_emulation.rs · mmio_write_emulation.rs
│   └── mmio_serial.rs · mmio_rtc.rs · mmio_block_identity.rs · mmio_block_queue.rs
├── probe_mmio/                # synthetic/diagnostic harness (NOT the real device modules)
│   ├── bus.rs · primecell.rs
│   ├── gic/ (distributor.rs · redistributor.rs · cpu_interface.rs · firmware_irq.rs)
│   └── virtio_block/ (device.rs · guest_memory.rs · request.rs · backend.rs · synthetic.rs · firmware.rs · probes.rs)
├── windows_arm/               # machine/firmware description + byte tooling
│   ├── constants.rs · platform_description.rs · fdt.rs · boot_disk.rs · uefi_handoff.rs · pflash.rs · reset_vector.rs
│   └── firmware/ (run_loop.rs · run_loop_probe.rs · diagnosis.rs · telemetry.rs · access_decode.rs · stage1.rs · diagnostic_vector.rs)
└── platform/                  # cfg-selected; ALL unsafe lives here after extraction
    ├── mod.rs                 # original mutually-exclusive cfg predicates, verbatim
    ├── apple/ (ffi.rs · host.rs · vm.rs · vcpu.rs · timer.rs · memory.rs · mmio.rs · pflash.rs · firmware.rs)
    └── unsupported/ (mod.rs · lifecycle.rs · mmio.rs · firmware.rs)
```

## 5. Staged extraction packets

Ordered safest / lowest-coupling first. Line ranges are baseline coordinates.
"Depends on" is packet number. Each row is one PR unless noted. Do **not**
combine adjacent high-risk rows just because they touch the same new file (in
particular never combine 30/31/42/43, read+write MMIO emulation, or the three
GIC components).

### Phase A — safe leaf modules (L1, zero `unsafe`, low risk)

| # | Packet | ~LOC | Source ranges | Target | Risk | Deps |
|---:|---|---:|---|---|---|---|
| 1 | HVF support + no-QEMU gate | 330 | 13957–14148; tests 28369–28427 | `support.rs`, `no_qemu_plan.rs` | Low | — |
| 2 | Declarative machine planning | 570 | 8257–8418, 11821–11917; tests 28428–28523 | `machine_plan.rs` | Low | 1 |
| 3 | Boot-disk / GPT layout | 1050 | 8419–8420, 8532–8552, 8803–8910, 11518–11632, 12544–12922, 13108–13195; tests 28703–28801 | `windows_arm/boot_disk.rs` | Low–Med | — |
| 4 | Static UEFI handoff + FV inspect | 1000 | 8553–8559, 8911–9028, 11228–11348, 12923–13107; tests 28802–28916 | `windows_arm/uefi_handoff.rs` | Low–Med | — |
| 5 | Static pflash slot planning | 730 | 9029–9036, 9166–9474, 11349–11484, 12947–13065; tests 28917–29026 | `windows_arm/pflash.rs` | Med | 4 |
| 6 | FDT blob codec | 1250 | 8453–8469, 8528–8531, 11961–12543 | `windows_arm/fdt.rs` | Med | — |
| 7 | Platform-description probe | 900 | 8427–8447, 8463–8464, 8560–8802, 11633–11960; tests 28524–28702 | `windows_arm/platform_description.rs` | Med | 2, 6 |

### Phase B — synthetic MMIO device models (L1, zero `unsafe`)

| # | Packet | ~LOC | Source ranges | Target | Risk | Deps |
|---:|---|---:|---|---|---|---|
| 8 | Synthetic MMIO bus | 320 | 4009–4064, 4245–4314; tests 33574–33588, 33646–33657, 34351–34361 | `probe_mmio/bus.rs` | Low–Med | — |
| 9 | Synthetic PL011/PL031 | 300 | 4067–4072, 4323–4420; tests 33589–33612 | `probe_mmio/primecell.rs` | Low–Med | 8 |
| 10 | GICv3 distributor + selection | 1050 | 4073–4118, 4148–4169, 4421–4954; tests 31485–31725 | `probe_mmio/gic/distributor.rs` | Med | 8 |
| 11 | GICv3 redistributor | 700 | 4955–5304; tests 31726–32036 | `probe_mmio/gic/redistributor.rs` | Med | 8, 10 |
| 12 | GIC CPU-interface + firmware IRQ | 1300 | 4119–4145, 6927–6964, 7480–7945; tests 32084–33004 | `probe_mmio/gic/cpu_interface.rs`, `firmware_irq.rs` | Med–High | 10, 11 |
| 13 | VirtIO guest-mem + request primitives | 800 | 5593–5870, consts 4216–4238; tests 33880–34023 | `probe_mmio/virtio_block/guest_memory.rs`, `request.rs` | Med | — |
| 14 | VirtIO block storage backends | 650 | 5871–6125; tests 34079–34350 (file/writable/flush/ISO) | `probe_mmio/virtio_block/backend.rs` | Med | 13 |
| 15 | Synthetic request seeding + runners | 800 | 6126–6688; tests 33880–34350 (synthetic/model/file) | `probe_mmio/virtio_block/synthetic.rs`, `probes.rs` | Med | 13, 14 |
| 16 | VirtIO-MMIO block device model | 1650 | 4170–4242, 5305–5592, 6689–6866, 7402–7479, 7946–8256; tests 32037–32083, 33005–33402, 33613–34078 | `probe_mmio/virtio_block/device.rs`, `firmware.rs` | High | 8, 10–15 |
| 17 | Public VirtIO block probe contracts | 1000 | 3442–4008; tests 34024–34350 (remainder) | `probe_mmio/virtio_block/probes.rs` | Low–Med | 14–16 |

### Phase C — firmware-diagnostics safe layer (L1, zero `unsafe`)

| # | Packet | ~LOC | Source ranges | Target | Risk | Deps |
|---:|---|---:|---|---|---|---|
| 18 | ARM exit/access decoders + names | 700 | 13434–13466, 13714–13989; tests 31371–31388, 32084–32142 | `windows_arm/firmware/access_decode.rs` | Low–Med | 12 |
| 19 | Low-vector post-repair telemetry | 1350 | 6867–7401, 13208–13433; tests 28045–28368, 29964–30276 | `windows_arm/firmware/telemetry.rs` | Med–High | 12, 18 |
| 20 | Firmware diagnosis + vector selection | 900 | 8448–8462, 8472–8527, 13467–13713, 13880–13956; tests 28102–28143, 30252–30276 | `windows_arm/firmware/diagnosis.rs`, `diagnostic_vector.rs` | Med | 18, 19 |
| 21 | Stage-1 walk + vector candidates | 2150 | 14311–14397, 15003–15971; inline 15628–15739 | `windows_arm/firmware/stage1.rs` | High | 20 |
| 22 | Diagnostic-vector route + repair | 1350 | 14707–15002, 15798–16056, 17698–18188; inline 14884–14922, 17973–18188 | `windows_arm/firmware/diagnostic_vector.rs` | High | 5, 20, 21 |

### Phase D — vertical probe extraction (public type + wrapper + both platform backends)

Each probe is extracted across all three seams at once (result type +
`render_text`, crate-root wrapper, supported/unsupported backend). Packets
23–40 (host-capabilities, vm-create, vcpu-create, vcpu-run+watchdog,
interrupt-timer, vtimer-exit, memory-map, pflash-HVF-map, reset-vector,
guest-entry, guest-exit-loop, mmio read-exit/read-emu/write-emu, PL011 serial
live, PL031 RTC live, virtio-block identity live, virtio-block queue live).
Source ranges span L1 (public type/tests), L2 (`platform/apple/*` backend), and
L3 (`platform/unsupported/*`). See the gpt-5.6-sol packet table (packets 23–40)
for exact per-probe ranges; risk is Med→High and each depends on its lifecycle
predecessor and the relevant Phase B/C seams. These move `unsafe` from L2 into
`platform/apple/{host,vm,vcpu,timer,memory,mmio,pflash,firmware}.rs`.

### Phase E — firmware run-loop and platform shells

| # | Packet | ~LOC | Target | Risk | Deps |
|---:|---|---:|---|---|---|
| 41 | Firmware IRQ/vtimer delivery helpers | 700 | `platform/apple/firmware.rs` + state in `probe_mmio/gic/firmware_irq.rs` | High | 12, 27–28 |
| 42 | Run-loop public contract + renderer | 2500 | `windows_arm/firmware/run_loop_probe.rs` | High | 12, 18–22, 41 |
| 43 | Live run-loop orchestrator | 3900 | `platform/apple/firmware.rs`, `platform/unsupported/firmware.rs`, thin `windows_arm/firmware/run_loop.rs` | Very High | 21–22, 26–31, 41–42 |
| 44 | cfg-selected platform shells + FFI decls | 250 | `platform/mod.rs`, `platform/apple/ffi.rs`+`mod.rs`, `platform/unsupported/mod.rs` | High | 23–43 |
| 45 | Remove exhausted root test module; finalize re-exports | ~0 | owning modules; optional `test_support.rs` | Low | 1–44 |

## 6. Isolating the 296 `unsafe` sites

All `unsafe` lives in L2 today and ends up under `platform/apple/` grouped by
responsibility, each with preserved safety invariants:

1. **`ffi.rs`** — the single `extern "C"` block, HV_* constants, `#[repr(C)]`
   `HvVcpuExit*` (14227–14433). Exact C signatures/widths/pointer-mutability;
   not a broad "unsafe utilities" module.
2. **`host.rs`** — capability out-parameter queries (14436–14489).
3. **`vm.rs` / `vcpu.rs`** — VM/vCPU create/destroy, register get/set,
   `hv_vcpu_run`, `hv_vcpus_exit`. No RAII conversion during extraction.
4. **`vcpu.rs`** — watchdog / cross-thread `hv_vcpus_exit` (16057–16115).
5. **`memory.rs`** — `hv_vm_allocate/map/unmap/deallocate` and slice
   construction; sizes, IPA ranges, permission bits, cleanup order unchanged.
6. **`pflash.rs`** — code/vars slot mapping, low-alias resolution, firmware
   byte copy, diagnostic-slot save/restore.
7. **`firmware.rs`** — raw `HvVcpuExit` reads, register/IRQ/vtimer interaction,
   context advance/restore, live guest-memory mapping.
8. **`mmio.rs`** — live read/write/serial/RTC/block probe guest-memory IO.

Pure checked byte-slice code (FDT, GPT, UEFI-FV, `VirtioGuestMemory`, virtq
parsing, synthetic bus/devices, telemetry, stage-1 arithmetic) must stay
`unsafe`-free — do not convert checked indexing to pointer arithmetic to share
code with the backend. Every `unsafe`-bearing PR generates a before/after site
map, asserts equal count for the moved responsibility, and reviews each block
body (reject any packet that widens a block, changes pointer lifetimes / map
permissions / alloc order, or turns an error into a success path).

## 7. Cross-cutting risks

1. **Public-API expansion** — use explicit `pub use` lists, never `pub mod`/
   wildcard for helpers (`MmioBus`, GIC models, FDT structs, backends stay
   private).
2. **The two `mod platform` blocks must stay mutually exclusive** — copy cfg
   predicates verbatim; check both configurations on every platform packet (a
   native build alone is insufficient).
3. **Vertical seams** — each probe = public type + wrapper + two backends; move
   all three together (Phase D) to avoid prolonged circular deps.
4. **Render strings are an external contract** — verbatim bodies + byte-for-byte
   assertions; watch field order, `Option` formatting, renamed local consts,
   `Debug` substitution, blocker precedence.
5. **Synthetic ≠ real devices** — keep the diagnostic models in `probe_mmio`,
   never fold them into the real `pl011`/`pl031`/`virtio_blk` modules.
6. **No premature "common platform abstraction"** — move both platform
   implementations exactly; no trait/macro/generic dispatch during extraction.
7. **File-I/O failure precedence** — preserve the existing order of
   missing-path / bad-size / open / read / validate / flush / cleanup failures.
8. **Ratchet is aggregate** — moving sites is not a reduction; lower a ceiling
   only in a separate PR that removes real duplication or `unsafe`.
9. **cfg-only name resolution** — inline children see `use super::*`;
   extraction makes deps explicit. Validate supported/unsupported/test/non-test/
   arch cfg branches; never "fix" an unused import by changing cfg behavior.
10. **Never combine cleanup with movement** — no RAII guards, const→binding
    swaps, string→enum, `PathBuf`→`&Path`, dedup of supported/unsupported, or
    watchdog-timing changes inside an extraction packet.

## 8. Success criterion

Clearer ownership and isolated review boundaries — not merely a smaller
root-file line count. At the end, `lib.rs` holds device-module declarations,
private extracted-module declarations, explicit re-exports preserving the
current public surface, and minimal glue; every `unsafe` site sits in a
`platform/apple/*` module named for its responsibility.
