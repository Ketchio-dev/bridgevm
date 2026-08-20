# Changelog

All notable changes to BridgeVM are documented in this file.

## [1.0.0] - 2026-08-20

Every release-blocking criterion in `capabilities/windows-hvf.json` is proven
by live evidence, including the final Windows closure gate
(driver-loadable image, host-driven resolution adoption, and the shipped
window-Coherence verbs answered live; receipt
`fefad903a56fa2f1c63e6067079208928d1dae1f415ff966611629ea37727a97`).

### Added

- A Mac-native Windows 11 Arm engine built directly on
  Hypervisor.framework, alongside the existing Apple Virtualization.framework
  and QEMU compatibility engines.
- Windows HVF support for persistent storage, display and input, networking,
  audio, guest integration, TPM/Secure Boot workflows, snapshots, and
  experimental 3D.
- An experimental Vulkan path and an experimental D3D11-compatible subset
  through virtio-gpu.
- Ad-hoc-signed `.dmg` and curl-installable `.tar.gz` packaging, with a
  documented macOS installation flow for builds distributed without Developer
  ID or notarization.
- A machine-readable Windows HVF capability registry backed by deterministic
  checks and dated live-gate evidence.

### Changed

- Defined the guest platform as a QEMU `virt`-compatible contract with
  documented deviations.
- Separated deterministic hosted checks from serialized, sealed-input live
  gates that require real Apple-silicon hardware or private Windows media.

### Security

- Guest-visible random values use the host CSPRNG and fail closed when entropy
  is unavailable.
- Release builds reject repository and `PATH` overrides for bundled helper
  binaries.
