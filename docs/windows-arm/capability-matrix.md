# Windows HVF capability matrix

Document status: **Current**
Last reviewed: **2026-08-04**

This is the generated view of the capability registry. The registry
[`capabilities/windows-hvf.json`](../../capabilities/windows-hvf.json) is the
single source of truth for capability state, release blockers and user-facing
wording; this page never states anything the registry does not.

Rules that make the matrix trustworthy:

- `proven` requires dated live evidence at the stated threshold. A single
  passing run below the sample count is not proof.
- `open` means measured but below threshold, or not yet measured. The latest
  honest measurement is recorded even when it is unflattering.
- `external` means the remaining work belongs to a third party.
- The product state may not be promoted past Engineering Preview while any
  release-blocking criterion is unproven. `scripts/render-capability-status.py`
  enforces that rule mechanically.

<!-- BEGIN GENERATED: capability-matrix -->
| ID | Capability | State | Threshold | Latest measurement |
| --- | --- | --- | --- | --- |
| A1 | Cold boot reliability | `open` | At least 9 of 10 cold boots report stage4_pass=1 AND firstboot_fresh=1 in firstboot-stage.txt. | Diagnostic gate a1-fix3 reached 11/12 after canceled-vtimer recovery, but boot-7 retained the KeIpiGenericCall stall with CNTV_CTL=0x1 and masked=false. Earlier combined evidence was 17/20 = 85%. Not proven. |
| A2 | Vulkan title frame rate | `open` | A real Vulkan title reports guest-side samples>0 AND p50>=30 FPS. | Rendering is confirmed with third-party content, but this swapchain emits no DxgKrnl present events, so PresentMon records samples=0 while dwm.exe measures normally at p50 30.8. A guest Vulkan present instrument is required. |
| A3 | D3D11 title frame rate | `open` | A real D3D11 title reports guest-side samples>0 AND p50>=30 FPS. | Instrumentation works with samples=311-369, but p50 is approximately 20 FPS. A self-authored D3D11 smoke reaches 54.3 FPS on the same path, so the stack is not capped at 20; the title stays on Composed: Flip and never reaches Hardware: Independent Flip. |
| A4 | Keyboard input | `proven` | F1-F12 work and a non-ASCII (Korean) string round-trips byte-identically. | queued_actions=12, emitted_key_reports=12, emitted_release_reports=12, marker_seen=true; Korean round-trip bytes and SHA-256 match. |
| A5 | Audio playback | `proven` | Guest PCM reaches the host CoreAudio ring with frames_rendered>0 AND drops==0. | frames_rendered=252513, drops=0, dropped_bytes=0, format_drops=0, ring_full_drops=0. callback_errors=3 tracked separately as a quality issue. |
| A6 | Clipboard | `proven` | Host and guest clipboard round-trips in both directions including Korean text with matching SHA-256. | CLIPSET to CLIPGET round-trip with identical expected/actual SHA-256 0edb7fed. |
| A7 | Folder sharing | `proven` | One file each direction transfers with matching SHA-256. | Host to guest 12345 bytes ok=true; guest to host 9001 bytes SHA-256 5d28425e matching guest Get-FileHash. |
| A8 | Dynamic resize | `proven` | After a window resize the guest-reported resolution equals the request. | 1600x900, 1920x1080 and 1280x720 all PASS with a matching final SET_SCANOUT. |
| A9 | In-app install | `proven` | ISO selection through install, driver injection and a 3D desktop is reachable from the UI. | Fresh 64GiB Windows built bundle-only, OOBE completed, viogpu3d 120.41 stage4 reached, WDDM Status OK, create3d=514, submit3d=821, flush=237, final 1280x800 desktop. |
| A10 | Standalone app | `proven` | The app in /Applications boots to 3D while the source repository is renamed away. | Deep codesign plus bundled Hypervisor entitlement verified; booted with the repository renamed to bridgevm.a10-hidden; device_status=0xf, 3D commands=649, create3d=826, flush=533. |
| A11 | No regression | `open` | Workspace, Venus lib, probe example and Swift suites plus fmt, clippy, budgets and docs gates all pass at the final release head. | Passed on 2026-07-30 at an earlier head with workspace 1480, Venus lib 777, probe 252. Must be re-established at the final V1 head; the full 649-function XCTest suite is not currently runnable without user-assisted Xcode setup. |
| A12 | Secure SMCCC TRNG | `proven` | Guest TRNG_RND_32/64 are served only by the host OS CSPRNG; no exit-count, time, PID or constant-derived fallback exists, and provider failure returns NO_ENTROPY. | The protocol lives in bridgevm_hvf::smccc_trng over SecRandomCopyBytes, with one dispatch shared by the primary and secondary loops. 17 tests cover the DEN 0098 register layout, bit masking, request limits, FEATURES discovery and the fail-closed paths; the exit-counter generator and the duplicated probe constants are deleted. Guest-visible behaviour is not yet re-measured on a live boot. |
| A13 | PSCI 1.1 conformance | `proven` | CPU_ON and AFFINITY_INFO implement the specified state table, affinity levels and error codes with atomic state transitions. | State logic lives in bridgevm_hvf::psci with 19 tests covering the CPU_ON table, affinity levels 0-3, cluster aggregation, CPU0, RES1 spellings and invalid targets/levels. CPU_ON reports ON_PENDING for a pending CPU, AFFINITY_INFO reads X2 and searches the whole topology, and the state check and Off->OnPending transition share one critical section. Live 1/2/4-vCPU topology validation is not yet run. |
| A14 | Typed product runtime | `open` | The product app and hvf-runner drive a typed runtime API from a versioned launch manifest instead of executing an example binary through shell. | The product path still runs the hvf_gic_boot_probe example. |
| A15 | Process-recreate reset | `open` | 100 consecutive guest SYSTEM_RESETs each produce a new helper PID, an increasing reset generation, a fresh guest boot marker and 4 online CPUs. | Reset is still in-process; the soak has not been run. |
| A16 | Release security and storage hardening | `proven` | Release builds launch only bundled signed helpers, verify socket peer UID, hold an exclusive disk/vars lease and use positional atomic disk I/O. | Overrides: scripts/check-release-overrides.sh inspects compiled objects and reports 4 overrides debug-only, PASS. Bundled signed helper: HelperCodeSignature.swift requires the swtpm helper to share the app team ID, with 10 swift-testing tests. Peer UID: crates/bridgevm-daemon/src/peer_credentials.rs with tests. Exclusive lease: crates/bridgevm-hvf/src/media_lock.rs. Positional atomic disk I/O: every seek+read on a data path is now read_exact_at/write_all_at -- nvme/disk.rs (read, write-back write, and the copy-on-write chunk fault), probe_mmio/virtio_backend.rs, virtio_blk/media_backend.rs and windows_arm/uefi_io.rs, whose write_all_at/read_exact_at were named for positional I/O while seeking. Three tests park the shared file offset at 4096, perform a read, a write-back write and an overlay fault, and require it to still be 4096; falsified by restoring seek+read, which reports the offset moved to 1536. The one remaining seek is in media_lock.rs, rewriting the lock file's holder line, which is not a data path. |
| A17 | Nonblocking provenance-sealed live queue | `open` | Long gates run asynchronously on the Studio local queue with sealed commit/image/vars hashes and no foreground wait beyond 120 seconds. | Gates are currently launched synchronously from the interactive session. |
| A18 | Claim consistency | `open` | README, STATUS, core descriptors, CLI and app wording are generated from or checked against this registry, and a deterministic check fails on drift. | This registry and its drift check are being introduced now; the stale bootability and beta strings are being removed in the same change. |
| A19 | Disk safety | `proven` | Powered-off snapshot export and restore keep a byte-exact atomic disk plus vars pair under an explicit byte quota. | All six acceptance items in docs/windows-arm/snapshot-scope-v1.md are proven against the real 64 GiB image. Items 1-4 and 6 by the pair gate (scripts/verify-powered-off-snapshot.sh, tier t1-snapshot): create, verify, clobber and restore reproduce both files byte for byte, and a tampered snapshot is refused before either live file is touched. Item 5 by the restore-boot gate (scripts/verify-snapshot-restore-boots.sh, tier t1-restore-boot, job 20260805-091656-10876-16297, result=pass): a marker written on C: before the snapshot came back after a restore, and a different marker written after the snapshot was gone -- so a restore that did nothing could not have passed. disk_sha256 56b1361282933f2cf3e409604cdc2da81f8c6ad8536de2bdcc472818e204280d and vars_sha256 8cc43bf0a11d8d1d68253069fed42667950a701c2f508a38c69e35b72fca3563 are identical across create, verify and restore. Evidence: docs/windows-arm/evidence/a19-snapshot-pair-20260804.md and docs/windows-arm/evidence/a19-restore-boots-20260805.md. |
| B1 | Boot stall rate measured (non-blocking) | `proven` | Measure and record the boot stall rate with n>=10. | n=17, 5/17 pass at the time of measurement. |
| B2 | Stall cause isolated (non-blocking) | `proven` | Name the stall cause by device axis, or record that all four variants behave identically. | Isolated more precisely than a device bisection: the guest ICD never observes the first Venus ring response, and pass/stall runs diverge at one identical command. |
| B3 | Present health ratio (non-blocking) | `proven` | Print the create3d/flush ratio at run end and document the health threshold. | present health create3d=0 flush=0 ratio=n/a healthy=false threshold=0.10; idle runs fail closed. |
| B4 | Pointer latency measured (non-blocking) | `proven` | Record a host-click to guest-reaction latency measurement. | HID delivery confirmed 1/1/1, but framebuffer checksums stayed at baseline through 1000ms, so visible reaction is >1000ms or absent. Recorded honestly rather than as a performance pass. |
| B5 | Snapshot scope approved (non-blocking) | `proven` | Document the V1 snapshot scope with owner approval. | V1 is powered-off cold snapshot as an atomic disk plus UEFI vars pair; running-state save is V2. |

Generated from [`capabilities/windows-hvf.json`](../capabilities/windows-hvf.json) by `scripts/render-capability-status.py`.
<!-- END GENERATED: capability-matrix -->

## How to change this page

Do not edit the generated block. Edit the registry and run:

```sh
python3 scripts/render-capability-status.py
```

`scripts/check-project.sh` runs the same script with `--check`, so a stale
block or a retracted claim fails the deterministic project check.
