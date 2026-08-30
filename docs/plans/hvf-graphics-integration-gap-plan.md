# Windows HVF graphics and integration roadmap

Document status: **Active plan**

Last revised: 2026-08-13

This document describes the remaining product work around display, graphics,
and host/guest integration for BridgeVM's Windows HVF engine. Historical product
comparisons have been removed; current capability claims come from the
[capability matrix](../windows-arm/capability-matrix.md).

## Current product surface

The Windows HVF path already has live evidence for:

- Windows 11 Arm desktop boot;
- virtio-gpu display with dynamic resolution;
- keyboard and absolute-pointer input;
- networking;
- host audio;
- clipboard and folder transfer through the guest agent;
- experimental Vulkan and D3D11-compatible 3D;
- restart/reset lifecycle and powered-off snapshots.

The next work is therefore not a basic "make the desktop usable" bring-up. It is
compatibility, frame pacing, distribution, recovery, and integration polish.

## Display roadmap

### Dynamic display behavior

Keep resolution and presentation state owned by an explicit guest/host contract.
A resize should have a bounded sequence:

```text
host window change
    -> requested guest mode
    -> guest acknowledges/applies mode
    -> virtio-gpu scanout changes
    -> host presents the new surface
```

Future work should cover:

- rapid repeated resizes;
- Retina/HiDPI scaling policy;
- full-screen transitions;
- multiple display modes and refresh rates;
- pointer mapping across dynamic geometry;
- clean fallback when the accelerated driver is not available.

### Presentation efficiency

The steady-state accelerated path should avoid full-frame synchronous CPU
readback. Readback remains useful for evidence and diagnostics, but normal
presentation should prefer shared/host-visible resources and asynchronous
scanout.

Measure:

- frame-time p50/p95/p99;
- time blocked in readback;
- dirty/damage region size;
- compositor overhead;
- host CPU usage while the guest is visually idle;
- resize and full-screen transition stalls.

## Graphics compatibility roadmap

The current experimental 3D path has passed narrow live gates. Broader preview
quality needs a compatibility matrix rather than a single demonstration title.

Suggested workload buckets:

1. native Windows Arm Vulkan applications;
2. x64 applications running through Windows translation;
3. D3D11 applications with different presentation models;
4. Chromium/Electron GPU acceleration;
5. media playback while audio and networking are active;
6. long-running 3D workloads with repeated window state changes.

For each workload, record:

- executable and driver identity;
- renderer path;
- startup success/failure;
- visible corruption;
- frame-time statistics;
- guest crash/driver reset events;
- clean shutdown and subsequent reboot.

A compatibility result belongs to the tested workload and build. It is not a
blanket claim for the API family.

## Guest integration roadmap

BridgeVM's guest-agent channel is the common control surface for features that
should not require a dedicated virtual hardware device each.

### Clipboard

Keep clipboard synchronization bounded by explicit message size, encoding, and
loop-prevention rules. Continue testing non-ASCII text and host/guest round trips.

### Folder transfer

The current transfer path should evolve toward predictable failure behavior:

- explicit maximum file/message sizes;
- atomic destination replacement where appropriate;
- clear partial-transfer cleanup;
- path traversal rejection;
- stable error reporting to the UI;
- throughput measurements on larger trees.

### Lifecycle

Guest-agent lifecycle commands should remain optional conveniences over a VM
that can still fail closed without the agent. Shutdown/restart requests need
bounded timeouts and a clear forced-stop path in the host UI.

### Time and desktop integration

Potential follow-on integration features include:

- time synchronization policy;
- richer host/guest file exchange;
- guest capability/version reporting;
- install/update state for BridgeVM guest components;
- diagnostics export from the guest agent.

Each new feature should extend the existing versioned protocol instead of
creating an unrelated host/guest side channel.

## Audio roadmap

The existing audio path should be hardened around:

- format changes;
- underrun/overrun accounting;
- host-device changes;
- VM pause/stop/restart;
- simultaneous audio + graphics load;
- long-run buffer growth and callback errors.

The audio gate is not just "a device enumerates"; actual guest PCM must reach the
host output path without silent loss.

## Recovery and reset

Graphics and integration features must survive the same process-generation
boundaries as the core VM.

On reset or process recreation:

- old virtio queues cannot signal the new generation;
- graphics mappings and renderer contexts are torn down;
- agent commands from the previous generation are not replayed accidentally;
- clipboard/share state re-anchors cleanly;
- audio resources close before the helper exits;
- durable disk/vars/vTPM state is flushed according to the runtime contract.

## Distribution

The ad-hoc-signed distribution has two distinct signing questions:

### macOS app distribution

The preview can be ad-hoc signed and distributed without Developer ID or
notarization. That is acceptable for a developer-focused preview as long as the
README explains the macOS trust override clearly and the distributed artifact
contains the project and third-party license notices.

### Windows guest drivers

A user-provided Windows ISO does not change the Windows kernel driver trust
model. Experimental test-signed graphics packages may require the automated
test-signing activation path. A production-signed driver remains a later
usability/distribution milestone, not a prerequisite for technical testers.

## Definition of a better preview

The next preview milestone should optimize for the following user experience:

1. download/build one BridgeVM DMG;
2. explicitly trust/open the ad-hoc-signed app on macOS;
3. select a user-provided Windows 11 Arm ISO;
4. create and install the VM from the app;
5. have required preview driver setup happen through the installer flow;
6. reach a resizable desktop with network, audio, clipboard, sharing, and the
   experimental 3D path;
7. recover cleanly when a driver or renderer path is unavailable;
8. export diagnostics that are useful without access to the source checkout.

That flow is more valuable at this stage than adding another graphics API claim
without improving installability or failure recovery.
