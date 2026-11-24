# BridgeVM current status

Document status: **Current**

This file is the concise evidence boundary for BridgeVM. Detailed measurements
live in the [capability matrix](docs/windows-arm/capability-matrix.md) and dated
receipts under `docs/windows-arm/evidence/`.

<!-- BEGIN GENERATED: capability-summary -->
**Product state: Engineering Preview.** Runs an installed Windows 11 Arm desktop on BridgeVM's own Hypervisor.framework VMM with persistent storage, display/input, network, audio, clipboard and folder integration, TPM/Secure Boot workflows, snapshots and window Coherence verbs. 3D acceleration is excluded from General Preview and v1; release-blocking evidence remains open and known defects are disclosed below.

Release scenarios met: **3 / 5**.

| ID | Scenario | State | Criteria | Known defects |
| --- | --- | --- | --- | --- |
| S1 | Get Windows running | `OPEN` | A9 (open), A1, A10 | **A9**: No retained clean-machine product-flow receipt yet proves either ISO installation or installed-disk import through the app. Both supported flows remain 3D-off. In the 3D-off configuration the app's live display window was choppy in recent hardware runs; its smoothness has not been measured since. Guest audio stuttered audibly: until 2026-11-03 every other 10 ms of Windows playback reached the host as silence, because the sound controller answered Windows' position reads at the 0x2084 alias with 0. Since that fix, 60-second live runs had 2-6 silent 10 ms blocks out of about 6,330, and under host load 0-8 blocks per run still repeated stale audio; at the v1.1.0 release head (2026-11-14) six such runs had 2-6 silent and 0-3 stale blocks out of about 6,400, and one of them had 2 host underrun callbacks. No listening check has confirmed the stutter is gone. |
| S2 | Use the Windows desktop | `OPEN` | A4, B4, A5, A8 | The Windows display follows the window size only after the one-time display driver install in the VM view (Red Hat virtio-win, Microsoft-signed, downloaded by the guest) and a VM restart; until then the resolution is fixed per VM before it starts (1280x800 by default, up to 2560x1600). The app window was choppy in earlier runs and its smoothness has not been measured since, and no listening check has confirmed that the audio stutter is gone (see A9). |
| S3 | Work with the Mac | `MET` | A6, A7 | none |
| S4 | Keep the VM safe | `MET` | A19, A15, A16, A12 | none |
| S5 | Go online | `MET` | A20 | none |

Release-blocking criteria proven: **13 / 14**. Open: A9.

Other known open defects:
- **B6**: Window title, tab and menu glyphs can be blank on the experimental Windows graphics path; body text alone does not prove glyph correctness.

- Graphics future path: Vulkan is a Graphics Lab future path, excluded from General Preview and v1; D3D11 compatibility is a Graphics Lab future path, excluded from General Preview and v1.
- Guest platform: QEMU virt-compatible guest contract with documented deviations.

State reviewed 2026-11-24 at commit `f34907451ebbb5d05550e70b73371dfc783d42b4`. This block is generated from [`capabilities/windows-hvf.json`](capabilities/windows-hvf.json) by `scripts/render-capability-status.py`.
<!-- END GENERATED: capability-summary -->

## How to read the generated status

The generated block is a **sealed capability snapshot**, not a statement that
arbitrary later commits inherit all of its evidence automatically.

The registry records the build on which each criterion was measured. After code,
packaging, or runtime changes, the final no-regression gate must be re-established
at the preview/release cut before that new head is treated as equally sealed.
Documentation-only history does not become live guest proof simply because it is
newer.

An interrupted replacement export can preserve both complete copies and ask for a
new output name. A legacy VM moved before storage-identity migration can need its
original location for recovery. [Snapshot recovery instructions](docs/app-cli-snapshot.md)

## Current development focus

Development centers on BridgeVM's own Hypervisor.framework VMM:

1. reproduce and repair CPU, interrupt/timer, device-I/O and shutdown/lifecycle
   defects with deterministic regressions;
2. keep the [guest machine contract](docs/machine-contract/qemu-virt.md) and its
   [documented deviations](docs/machine-contract/qemu-virt-deviations.json) explicit;
3. improve audio and display continuity and keep the measured snapshot recovery
   behavior intact;
4. validate the exact source revision in hosted CI, then use sealed physical-Mac
   campaigns for claims that require a real guest;
5. complete clean-machine install/import evidence and re-establish the final
   no-regression gate before a preview or release cut.

The [contribution guide](CONTRIBUTING.md#choose-the-right-area) maps these areas
to source and focused checks. [Dated engineering records](docs/history/windows-hvf/)
preserve individual findings; they do not promote product capability. The app's
Apple VZ and QEMU engines were removed; a VM saved with one is listed as no
longer supported and its files are left untouched.

## Distribution

The General Preview is published (latest `v0.1.0`); see the
[installation guide](docs/install.md). The terminal installer
requires a `BridgeVM-release.json` contract and refuses releases without it,
even when an archive checksum is valid.

General Preview is the user-facing channel: 3D-off, no bundled Windows kernel
package, no TESTSIGNING requirement, and no weakening of Secure Boot policy.
Users supply licensed Windows media and the required signed ARM64 driver
payload. Graphics Lab is a separate test-signed workflow on disposable guests;
it cannot satisfy A9. Windows rejection of test mode remains an explicit error.
See the [distribution channel contract](docs/distribution-channels.md).

The Mac app uses ad-hoc signing, without Developer ID signing or Apple
notarization. A DMG downloaded in a browser needs the one-time Open Anyway
step; the copy the terminal installer fetches does not. Windows driver
trust is a separate constraint; owning an ISO does not establish it. Packaging,
update/rollback UX and stronger artifact provenance remain development work.

## Known limitations

The Engineering Preview boundary is the proven evidence set, not universal
compatibility:

- driver setup and recovery remain developer-oriented;
- ad-hoc Mac distribution requires a one-time Open Anyway override for a DMG
  downloaded in a browser;
- clean-machine coverage is smaller than a mature VM product needs, and the
  declared host matrix (B8) covers only the M4 and M5 Apple-silicon
  generations this project can run on; M1, M2 and M3 are uncovered, not
  implied to work;
- update/rollback UX is not yet a stable public contract;
- running-state suspend is intentionally outside the current v1 persistence
  scope;
- some historical tools and evidence paths are still more lab-oriented than
  product-oriented.

## Evidence discipline

BridgeVM ranks evidence as in [`CONTRIBUTING.md`](CONTRIBUTING.md#evidence-levels):

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
