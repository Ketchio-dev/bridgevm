# BridgeVM current status

Document status: **Current**

Last revised: 2026-08-20

This file is the concise evidence boundary for BridgeVM. Detailed measurements
live in the [capability matrix](docs/windows-arm/capability-matrix.md) and dated
receipts under `docs/windows-arm/evidence/`.

<!-- BEGIN GENERATED: capability-summary -->
**Product state: 1.0.** Runs an installed Windows 11 Arm desktop on BridgeVM's own Hypervisor.framework VMM with persistent storage, display/input, dynamic resolution, network, audio, clipboard and folder integration, TPM/Secure Boot workflows, snapshots, window Coherence verbs and experimental 3D. Every release-blocking criterion is proven by live evidence; known open defects are disclosed below.

Release-blocking criteria proven: **19 / 19**. Open: none.

Known open defects:
- **B4**: Pointer clicks are unreliable: in the latest 10-boot live batch the host delivered every click but Windows acted on only 1 of 10. Keyboard input is unaffected. Tracked as criterion B4.

- Graphics: Experimental Vulkan path and Experimental D3D11-compatible subset.
- Guest platform: QEMU virt-compatible guest contract with documented deviations.

State reviewed 2026-08-20 at commit `9f7d57a2b6dcaaefb98332fabbab59c932b6dce1`. This block is generated from [`capabilities/windows-hvf.json`](capabilities/windows-hvf.json) by `scripts/render-capability-status.py`.
<!-- END GENERATED: capability-summary -->

## How to read the generated status

The generated block is a **sealed capability snapshot**, not a statement that
arbitrary later commits inherit all of its evidence automatically.

The registry records the build on which each criterion was measured. After code,
packaging, or runtime changes, the final no-regression gate must be re-established
at the preview/release cut before that new head is treated as equally sealed.
Documentation-only history does not become live guest proof simply because it is
newer.

## Engine overview

| Engine | Current boundary |
| --- | --- |
| **Windows HVF** | Custom Hypervisor.framework VMM; installed Windows 11 Arm desktop, persistent storage, SMP, display/input, network, audio, guest integration, security lifecycle, snapshots, and experimental 3D |
| **Apple VZ** | Working narrow Linux/macOS Arm path through Virtualization.framework |
| **Compatibility** | Supervised QEMU path for broad guest support and emulation |

The custom Windows engine is the main engineering focus.

## Windows HVF: proven product surfaces

The capability registry currently contains evidence for the following release
criteria.

### Boot and runtime

- fresh first-boot reliability meeting the fixed cold-boot threshold;
- 4-vCPU Windows execution;
- guest restart and process-recreate reset lifecycle;
- 100 consecutive reset/recreate cycles on the proven configuration;
- clean guest shutdown and storage writeback;
- standalone packaged-app boot without relying on the source checkout.

### Storage and firmware

- persistent NVMe-backed Windows disk;
- persistent UEFI variables;
- powered-off snapshot create/verify/restore as an atomic disk + vars pair;
- restored snapshot boot evidence proving that the restored guest state actually
  returned.

### Display, input, and integration

- virtio-gpu display;
- keyboard input (absolute-pointer *delivery* is proven, but click reliability
  is an open defect — see criterion B4 and the known limitations below);
- non-ASCII text path;
- dynamic resize;
- network connectivity;
- host audio playback from guest PCM;
- clipboard round trip;
- folder transfer;
- guest-agent lifecycle/control path.

### Security

- host-CSPRNG-backed guest TRNG implementation;
- TPM 2.0 TIS/PPI command path;
- live PPI clear operation;
- Secure Boot guest evidence;
- measured-boot event retrieval;
- encrypted vTPM state with Keychain-associated VM identity;
- authenticated recovery/migration lifecycle;
- fail-closed handling for missing security-state provenance.

### Experimental graphics

- live Vulkan workload evidence above the configured frame-rate threshold;
- live D3D11 workload evidence above the configured threshold across the required
  campaign;
- guest driver/ICD identity checks as part of the graphics evidence;
- host-side renderer and virtio-gpu 3D command traces.

These are intentionally **experimental graphics capabilities**. A passing title
or smoke does not imply universal game/API compatibility.

## Distribution

BridgeVM does not need Developer ID signing or Apple notarization to remain
usable. The repository has an ad-hoc-signing development path, and the packaging
path produces a DMG without paid Apple credentials.

Users may need to explicitly trust/open the downloaded app in macOS.
This is a distribution/trust UX limitation, not a VMM execution requirement.

BridgeVM does not redistribute Windows. Users supply their own Windows 11 Arm
media and license.

Experimental Windows graphics packages can have a separate signing constraint.
The current test-driver activation flow stages the package and performs live
Windows TESTSIGNING/certificate/bind/reboot steps. Windows may reject enabling
test mode when Secure Boot policy prevents the BCD change; that case must remain
an explicit error instead of silently weakening security.

Production Windows driver signing remains a future usability/distribution
milestone for users who should not need test mode. It is not made unnecessary by
using a user-provided ISO, because ISO ownership and kernel-driver trust are
separate concerns.

## Known limitations in 1.0

The 1.0 boundary is the proven evidence set, not universal compatibility:

- GPU compatibility is much narrower than "all Vulkan/D3D11 software";
- driver setup and recovery remain developer-oriented;
- ad-hoc Mac distribution requires a user trust override for downloaded builds;
- clean-machine coverage is smaller than a mature VM product needs;
- update/rollback UX is not yet a stable public contract;
- running-state suspend is intentionally outside the current v1 persistence
  scope;
- some historical tools and evidence paths are still more lab-oriented than
  product-oriented;
- **pointer clicks are unreliable** (criterion B4, open): in the latest live
  batch the host delivered every click but the guest acted on 1 of 10
  (`docs/windows-arm/evidence/b4-pointer-batch-20260817.md`). Every
  host-visible counter is identical between a landed and a lost click, so the
  next step is guest-side instrumentation. Keyboard input is unaffected;
- the legacy `.qemuCompat` engine runs swtpm, qemu-system-aarch64 and the edk2
  firmware from fixed Homebrew paths, so on that path anything a user places
  there decides what the app runs and what the guest boots. That is how the
  backend is meant to work -- it refuses to start when those files are absent --
  but it is still outside the signed bundle, which the HVF and Apple VZ paths
  are not. `scripts/check-release-overrides.sh` records all three as known
  violations and fails if any is fixed without the record being removed.

## Current priorities

The highest-value work from this point is:

1. keep the ad-hoc DMG deterministic, license-complete, and easy to verify;
2. keep README/status/documentation consistent with the capability registry;
3. simplify user-supplied Windows ISO installation and driver setup;
4. make test-signing/Secure Boot conflicts explicit in the UI and diagnostics;
5. broaden real application compatibility and collect frame-time rather than
   average-FPS-only data;
6. run longer graphics/reset/resize/recovery soak coverage;
7. improve clean-machine install, diagnostics export, and recovery UX;
8. re-run the final no-regression gate at each preview tag/head.

Developer ID/notarization, production Windows driver signing, and stronger
artifact provenance can be added later without blocking technical users from
trying the preview now.

## Evidence discipline

BridgeVM uses the hierarchy defined in [`AGENTS.md`](AGENTS.md):

1. live gate receipts on real hardware;
2. live single runs;
3. automated tests;
4. static reasoning.

A lower evidence level never silently promotes a higher-level claim. Failed
experiments remain in history. Thresholds are not lowered to fit the result.

## Verify the repository

Run the deterministic project check:

```sh
scripts/check-project.sh
```

Useful individual checks include:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo +1.85.0 check --workspace --locked
scripts/check-refactor-budgets.sh
bash scripts/check-documentation-system.sh
```

Real Windows boots, graphics workloads, Hypervisor.framework behavior, and
clean-machine distribution behavior require the corresponding live/host gates;
hosted CI is not a substitute.

## Sources of truth

- [`capabilities/windows-hvf.json`](capabilities/windows-hvf.json) — capability
  wording and criterion state;
- [capability matrix](docs/windows-arm/capability-matrix.md) — thresholds and
  measurements;
- [documentation index](docs/README.md) — current vs plan vs evidence files;
- [`THIRD-PARTY-NOTICES.md`](THIRD-PARTY-NOTICES.md) — third-party licensing and
  redistribution boundaries.
