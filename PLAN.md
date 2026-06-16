# Final Development Plan: Open-Source Parallels-Class Virtualization App

## Working name

Use a neutral project name that does not sound like Parallels, VMware, or UTM.

For this plan, I’ll call it **BridgeVM**.

## Core product thesis

**BridgeVM should not be “another QEMU GUI.”**
It should be a **two-engine virtualization app**:

1. **Fast Mode**
   A lightweight, Mac-optimized, Parallels-like path for supported modern guests.

2. **Compatibility Mode**
   A heavier, QEMU-powered, expert path for legacy OSes, x86 emulation, unusual hardware, and advanced configuration.

The central idea is:

> **Fast Mode should feel like Parallels. Compatibility Mode should feel like UTM/QEMU, but with a better UI.**

This gives the product a strong identity. Users who want speed get a constrained, polished experience. Users who need full control get an advanced compatibility engine.

---

# 1. Why the two-mode strategy is correct

Trying to make one engine do everything will make the app heavy. Parallels feels light because it is optimized for a narrow set of high-value use cases: modern Windows, Linux, and macOS guests on Mac hardware, with deep guest tools and strong macOS integration.

QEMU is excellent, but it is a general-purpose machine emulator and virtualizer. QEMU’s own documentation describes system emulation as a full virtual machine model with CPUs, memory, emulated devices, hardware accelerators, and TCG software emulation for many CPUs. That flexibility is powerful, but it naturally creates complexity and overhead. ([QEMU][1])

So BridgeVM should split the product like this:

```text
BridgeVM
├── Fast Mode
│   ├── narrow OS support
│   ├── low overhead
│   ├── native macOS UX
│   ├── Metal display path
│   ├── BridgeVM Guest Tools
│   └── automatic resource management
│
└── Compatibility Mode
    ├── wide OS support
    ├── QEMU backend
    ├── emulation support
    ├── advanced hardware settings
    ├── legacy OS support
    └── expert/debug workflows
```

The mistake to avoid is making **Fast Mode** just “QEMU with fewer settings.”
Fast Mode must eventually be a separate execution path.

---

# 2. User-facing mode names

Do **not** call them “light virtualization” and “real virtualization” in the UI. Both are real virtualization. Calling one “real” makes the other sound fake.

Use this naming:

| Public name            | Internal name | Meaning                                |
| ---------------------- | ------------- | -------------------------------------- |
| **Fast Mode**          | `LightVM`     | Lightweight, optimized, Parallels-like |
| **Compatibility Mode** | `FullVM`      | Full QEMU-style compatibility          |

The VM creation screen should say:

```text
Choose how you want to run this virtual machine.

Fast Mode
Best for speed, battery life, and Mac-like integration.
Recommended for Windows 11 Arm, Linux Arm64, and macOS guests.

Compatibility Mode
Best for advanced users, legacy operating systems, x86 emulation, and custom hardware.
May be slower or use more battery.
```

Default behavior should be automatic:

```text
User chooses OS first.
BridgeVM recommends the correct mode.
Advanced users can override the mode.
```

---

# 3. Final product positioning

BridgeVM should be positioned as:

> **An open-source Mac virtualization app with two engines: a lightweight Parallels-like engine for supported systems, and a full QEMU compatibility engine for everything else.**

Not:

> “Open-source Parallels clone.”

Not:

> “A prettier QEMU frontend.”

Not:

> “A free VMware replacement.”

The strongest positioning is:

> **The fastest open-source way to run Windows 11 Arm, Linux Arm64, and macOS VMs on Apple Silicon, with a compatibility mode for advanced use cases.**

---

# 4. Target platform strategy

## Phase 1 primary target

Start with:

```text
Host:
Apple Silicon Mac

Primary guests:
Windows 11 Arm
Ubuntu Arm64
Fedora Arm64
Debian Arm64
macOS Arm guest

Secondary guests:
Other Arm64 Linux distributions
```

This is the only realistic way to chase Parallels-level lightness.

Apple’s Virtualization framework provides high-level APIs for creating and managing VMs on Apple Silicon and Intel Macs, and Apple also documents macOS and Linux VM workflows through this framework. ([Apple Developer][2]) Apple’s Hypervisor framework is lower-level and is used to create and control hardware-assisted VMs and vCPUs from user-space. ([Apple Developer][3])

## Phase 2 secondary hosts

After macOS is strong:

```text
Linux host:
KVM/QEMU/libvirt backend

Windows host:
QEMU/WHPX backend
```

QEMU’s WHPX backend uses Windows Hypervisor Platform for hardware acceleration on Windows hosts, including x86_64 and Arm64 Windows machines. ([QEMU][4]) libvirt can manage multiple virtualization platforms, including KVM, QEMU, Hypervisor.framework, Xen, and others, and it targets Linux, FreeBSD, Windows, and macOS. ([libvirt.org][5])

---

# 5. High-level architecture

```text
BridgeVM.app
Native macOS UI
SwiftUI/AppKit
Thin, fast, mostly presentation logic
        │
        ▼
bridgevmd
Rust core daemon
VM lifecycle, mode selection, resources, storage, network, API
        │
        ├──────────────────────────────────────┐
        ▼                                      ▼
Fast Mode Engine                         Compatibility Engine
LightVM                                  FullVM
Apple VZ / custom HVF path               QEMU / HVF / TCG path
        │                                      │
        ▼                                      ▼
displayd                                qemu-display adapter
Metal compositor                         SPICE/VNC/QEMU display fallback
        │
        ▼
BridgeVM Guest Tools
Windows service / Linux daemon / macOS guest helper
Clipboard, resolution, sharing, drag-drop, app/window metadata
```

The project should be built as a collection of small, isolated components:

| Component              | Responsibility                                    |
| ---------------------- | ------------------------------------------------- |
| `BridgeVM.app`         | Native UI, VM dashboard, setup wizard, settings   |
| `bridgevmd`            | Core daemon, VM lifecycle, policies, API          |
| `lightvm-runner`       | Fast Mode VM runner                               |
| `fullvm-runner`        | QEMU Compatibility Mode runner                    |
| `displayd`             | Metal-based display pipeline                      |
| `networkd`             | metadata-only network plans for NAT, port forwarding, host-only, isolated, and bridged intents |
| `storaged`             | disk images, snapshots, compaction, export/import |
| `agentd`               | host-side guest tools protocol                    |
| `bridgevm-tools-win`   | Windows guest tools                               |
| `bridgevm-tools-linux` | Linux guest tools                                 |
| `bridgevm` CLI         | Developer automation                              |

---

# 6. Fast Mode architecture

Fast Mode is the heart of the product.

## 6.1 Purpose

Fast Mode should be optimized for:

```text
low CPU idle usage
fast suspend/resume
fast display updates
low battery impact
simple setup
automatic resources
Mac-like integration
strong guest tools
minimal advanced settings
```

The goal is not maximum compatibility. The goal is **excellent experience inside a narrow supported matrix**.

## 6.2 Supported Fast Mode guests

Initial Fast Mode support:

| Guest           | Priority | Backend                                                 |
| --------------- | -------: | ------------------------------------------------------- |
| Ubuntu Arm64    |       P0 | Apple Virtualization.framework                          |
| Fedora Arm64    |       P0 | Apple Virtualization.framework                          |
| Debian Arm64    |       P1 | Apple Virtualization.framework                          |
| macOS Arm guest |       P1 | Apple Virtualization.framework                          |
| Windows 11 Arm  |    P0/P1 | Initially restricted QEMU/HVF, long-term custom HVF VMM |

Windows support needs special caution. Microsoft’s official support page documents Parallels Desktop versions 18, 19, and 20 as authorized solutions for running Arm versions of Windows 11 Pro and Enterprise on Apple M1, M2, and M3 computers. BridgeVM should not claim the same Microsoft-authorized status unless such authorization is actually obtained. ([Microsoft 지원][6])

## 6.3 Fast Mode restrictions

Fast Mode should deliberately refuse or hide:

```text
x86_64 guest OS boot
legacy BIOS
old Windows versions
custom chipset selection
arbitrary QEMU device injection
random PCI devices
manual CPU model selection
exotic network cards
unbounded USB passthrough
```

This restriction is a feature, not a weakness.

Fast Mode should tell the user:

```text
This operating system is not eligible for Fast Mode.
Use Compatibility Mode instead.
```

## 6.4 Fast Mode backend plan

### Stage 1: Apple Virtualization.framework for Linux/macOS

Use Apple Virtualization.framework for Linux Arm64 and macOS Arm guests. This lets BridgeVM avoid pulling in the full QEMU stack for the first high-performance path. Apple’s Virtualization framework is the correct starting point for high-level macOS-native VM creation and management. ([Apple Developer][2])

### Stage 2: restricted QEMU/HVF for Windows 11 Arm

At first, Windows 11 Arm should use a heavily controlled QEMU/HVF path.

The user should not see QEMU.
The product should expose it as Fast Mode Experimental.

Internally:

```text
Windows 11 Arm Fast Mode v0
├── QEMU
├── HVF acceleration
├── fixed virtual hardware profile
├── UEFI
├── TPM support
├── virtio storage/network
├── BridgeVM Tools
└── Metal display adapter where possible
```

### Stage 3: custom HVF-based Windows Arm VMM

Long-term, if the goal is truly Parallels-class lightness, Windows 11 Arm needs a dedicated fast path that gradually replaces QEMU devices with BridgeVM-owned devices.

Target:

```text
Windows 11 Arm Fast Mode v2
├── Hypervisor.framework
├── custom VMM
├── minimal virtio device model
├── custom display path
├── custom shared-folder channel
├── custom guest tools protocol
└── Metal compositor
```

This is a hard systems project, but it is the right direction.

---

# 7. Compatibility Mode architecture

Compatibility Mode is the expert engine.

## 7.1 Purpose

Compatibility Mode should support:

```text
x86_64 Windows
x86_64 Linux
older Windows versions
BSD
custom kernels
RISC-V experiments
legacy BIOS
UEFI
QEMU device configuration
manual QEMU args
serial console
research OSes
low-level debugging
```

## 7.2 Backend

Use QEMU as the primary Compatibility Mode engine.

QEMU already supports multiple hardware acceleration paths and TCG software emulation; this is exactly what Compatibility Mode needs. ([QEMU][1])

On macOS:

```text
QEMU + HVF when possible
QEMU + TCG when emulation is required
```

On Linux:

```text
QEMU + KVM
libvirt integration later
```

On Windows:

```text
QEMU + WHPX
```

## 7.3 Compatibility Mode display

Use layered display support:

```text
Tier 1:
BridgeVM display adapter, where possible

Tier 2:
SPICE

Tier 3:
VNC/QEMU fallback
```

Compatibility Mode does not need to feel as light as Fast Mode. Its promise is:

> “It may be heavier, but it can run far more things.”

## 7.4 Advanced settings

Compatibility Mode should expose:

```text
machine type
firmware
chipset
CPU model
accelerator
RAM
disk bus
network adapter
display adapter
USB controller
serial console
TPM
QEMU arguments
boot order
kernel/initrd direct boot
cloud-init
snapshot chain
```

Fast Mode should hide most of these.

---

# 8. Mode selection logic

The user should not have to understand virtualization theory.

The VM wizard should work like this:

```text
Step 1: Choose operating system
Step 2: Choose installation source
Step 3: BridgeVM recommends a mode
Step 4: User accepts or opens advanced settings
```

Example decisions:

| User choice                    | Recommended mode   | Message                                  |
| ------------------------------ | ------------------ | ---------------------------------------- |
| Windows 11 Arm                 | Fast Mode          | Best performance on Apple Silicon        |
| Ubuntu Arm64                   | Fast Mode          | Native optimized path available          |
| Fedora Arm64                   | Fast Mode          | Native optimized path available          |
| macOS guest                    | Fast Mode          | Apple-native VM path available           |
| Windows 10 x86_64              | Compatibility Mode | Requires x86 emulation or legacy support |
| Windows 7                      | Compatibility Mode | Legacy OS                                |
| Ubuntu x86_64 on Apple Silicon | Compatibility Mode | Emulation may be slow                    |
| FreeBSD                        | Compatibility Mode | Advanced OS support                      |
| RISC-V Linux                   | Compatibility Mode | CPU emulation required                   |

The app should display clear performance expectations:

```text
Fast Mode:
Expected performance: High
Battery impact: Low
Integration: Full

Compatibility Mode:
Expected performance: Medium to low
Battery impact: Higher
Integration: Limited or partial
```

---

# 9. Guest Tools strategy

This is the most important part of the entire product.

Parallels-level experience does not come only from the hypervisor. It comes from the guest integration layer. Parallels Tools includes features such as dynamic resolution, shared folders, clipboard synchronization, and Coherence-related integration. ([Parallels Documentation][7]) Parallels Coherence mode allows Windows apps to run on the Mac as though they were native Mac apps, without managing two separate desktops. ([Parallels Knowledge Base][8])

BridgeVM therefore needs its own tools:

```text
BridgeVM Tools for Windows
BridgeVM Tools for Linux
BridgeVM Tools for macOS guest
```

## 9.1 BridgeVM Tools features

| Feature                |       Linux |       Windows | Priority |
| ---------------------- | ----------: | ------------: | -------: |
| Heartbeat              |         Yes |           Yes |       P0 |
| Guest IP reporting     |         Yes |           Yes |       P0 |
| Time sync              |         Yes |           Yes |       P0 |
| Dynamic resolution     |         Yes |           Yes |       P0 |
| Clipboard text sync    |         Yes |           Yes |       P0 |
| File sharing helper    |         Yes |           Yes |       P0 |
| Drag and drop          |         Yes |           Yes |       P1 |
| App launcher           |         Yes |           Yes |       P1 |
| Window metadata        |         Yes |           Yes |    P1/P2 |
| Tray/menu integration  |     Partial |           Yes |       P2 |
| Coherence-like support | Linux first | Windows later |    P2/P3 |
| Agent auto-update      |         Yes |           Yes |       P1 |
| Crash diagnostics      |         Yes |           Yes |       P1 |

## 9.2 Guest Tools protocol

Use a simple, versioned protocol:

```text
Transport:
vsock or virtio-serial

Encoding:
current alpha newline-delimited serde JSON envelopes;
future protobuf or Cap’n Proto remains open

Host service:
agentd

Guest services:
bridgevm-tools-win
bridgevm-tools-linux
```

Protocol messages:

```text
GuestHello with agent version, advertised capabilities, and tools-token auth
Heartbeat
TimeSync
ClipboardChanged
SetClipboard
ResizeDisplay
MountShare
UnmountShare
FileDropStart
FileDropChunk
FileDropComplete
ListApplications
LaunchApplication
ListWindows
FocusWindow
CloseWindow
GuestMetrics
CommandResult
AgentUpdateAvailable
```

## 9.3 Security model

Guest Tools are powerful and risky. Design them with a strict permission model.

Default:

```text
clipboard sync: enabled
shared folders: user-approved only
drag-and-drop: enabled after confirmation
host command execution: disabled
guest command execution: disabled by default
auto-update: signed packages only
```

Each VM gets its own tools auth token. The first `GuestHello` must prove
possession of that token and advertise the agent version plus supported feature
capability names before host commands such as clipboard, resize, or
shared-folder mount are trusted. The wire shape keeps auth optional for
forward-compatible decoding, but a validated `GuestHello` requires it.
`bridgevm-agentd` starts as the reusable host-side library that turns an
untrusted `AgentEnvelope` into a validated session by checking that token,
rejecting unknown capabilities, and rejecting capability versions newer than VM
policy allows; the eventual `agentd` process will use that same gate. After the
session is accepted, host command routing checks each message against advertised
capabilities such as `clipboard`, `display-resize`, `shared-folders`,
`applications`, `windows`, `guest-ip`, `time-sync`, `guest-metrics`, and
`agent-update`. Host
commands with a `request_id` become pending until a matching `CommandResult`
arrives; unexpected results and duplicate pending request IDs are rejected.
The manifest `sharedFolders` list is the durable approved-path policy surface:
CLI/API/daemon and macOS dashboard list/add/remove operations may change that
policy, but they do not live-mount a share in the guest. A mount remains an
explicit guest-tools command that resolves an approved manifest name to an
opaque host path token and sends only that token to the authenticated guest
tools session.

Example:

```yaml
integration:
  clipboard:
    enabled: true
    directions: bidirectional
  shares:
    requireUserApprovedPaths: true
  guestCommands:
    enabled: false
  agentUpdates:
    requireSignature: true
```

---

# 10. Display and graphics plan

Display latency is one of the biggest reasons a VM feels heavy. Even if CPU performance is high, the user will think the VM is slow if mouse movement, scrolling, typing, or window resizing feels delayed.

## 10.1 Fast Mode display pipeline

Fast Mode needs a native display compositor:

```text
guest framebuffer
→ dirty region detection
→ shared memory transport
→ Metal texture update
→ CoreAnimation layer
→ cursor overlay
→ adaptive frame pacing
```

Rules:

```text
Do not repaint at 60 FPS when nothing changes.
Do not render the guest cursor inside the framebuffer if host cursor overlay is possible.
Do not use VNC as the main Fast Mode display path.
Throttle background VM rendering.
Handle Retina scaling precisely.
```

## 10.2 Display performance budgets

Target metrics:

| Scenario                         |                   Target |
| -------------------------------- | -----------------------: |
| UI idle CPU                      |                  near 0% |
| VM idle CPU with visible desktop |        under 1–2% target |
| VM paused CPU                    |                  near 0% |
| Clipboard sync                   |      under 100 ms target |
| Resize response                  |       visually immediate |
| Resume from suspend              | under 3–5 seconds target |
| Background VM display FPS        | adaptive, often 0–10 FPS |
| Foreground productivity VM       |       30–60 FPS adaptive |

These are product goals, not guaranteed initial results.

## 10.3 Windows graphics reality

Do not promise gaming performance early.

UTM’s official site states that UTM does not currently support Windows GPU emulation/virtualization and therefore lacks Windows 3D acceleration such as OpenGL and DirectX; it also describes Linux Virgl support as experimental. ([UTM][9]) That is a useful warning: Windows graphics acceleration is one of the hardest parts of this market.

BridgeVM should focus first on:

```text
Office apps
browsers
messaging apps
business apps
accounting/ERP apps
VS Code
light developer tools
2D productivity workflows
```

Do not start with:

```text
DirectX 12 gaming
high-end CAD
GPU compute
anti-cheat games
professional 3D workloads
```

## 10.4 Long-term graphics R&D

Long-term options:

```text
custom virtual GPU
Windows WDDM driver
Direct3D translation to Metal
VirGL/Venus/gfxstream research
RDP RemoteApp-style fallback
window-region streaming
```

This should be treated as a multi-year R&D track.

---

# 11. Coherence-like mode plan

Coherence-like functionality should not be an MVP feature. It should be built in stages.

## Stage 1: App Launcher

Host shows guest apps.

```text
User clicks Windows Excel in BridgeVM launcher
→ Guest agent launches Excel inside Windows
→ App appears inside normal VM window
```

## Stage 2: Dock integration

Guest apps appear in a BridgeVM-controlled Dock-like list or macOS Dock proxy.

```text
Windows app icon
app name
running state
launch/focus/quit
```

## Stage 3: Window metadata

Guest Tools report:

```text
window title
process name
app icon
window bounds
minimized state
focused state
PID
```

Host can focus or close guest windows through the agent.

## Stage 4: Pseudo-Coherence

Host crops guest window regions and displays them as separate macOS windows.

```text
guest desktop hidden
guest window regions captured
host NSWindow created per guest window
input coordinates translated back to guest
```

This is not perfect, but it can feel good enough for productivity apps.

## Stage 5: True Coherence

Long-term:

```text
real host window mapping
Mission Control behavior
Dock behavior
Cmd+Tab integration
multi-monitor correctness
drag-and-drop between native and guest apps
IME correctness
Retina scaling
guest tray/menu bar integration
```

This is one of the hardest features in the entire plan.

---

# 12. Storage and file sharing

## 12.1 VM bundle layout

Each VM should be stored as a readable bundle:

```text
Windows Work.vmbridge/
├── manifest.yaml
├── disks/
│   ├── root.qcow2
│   └── snapshots/
├── nvram/
├── tpm/
├── logs/
├── screenshots/
├── agent/
└── metadata/
```

Fast Mode can use a simpler optimized disk format where appropriate. Compatibility Mode should use qcow2 by default.

## 12.2 Shared folders

Use different implementations per mode.

Fast Mode:

```text
optimized shared folder path
virtiofs for Linux where possible
custom Windows sharing helper long-term
BridgeVM Tools integration
```

Compatibility Mode:

```text
virtiofs where available
9p fallback
SMB fallback
SPICE/webdav fallback where appropriate
```

Virtiofs is specifically designed to let VMs access a host directory tree while aiming for local file-system semantics and performance, making it a strong foundation for Linux shared folders. ([libvirt.org][10])

## 12.3 Snapshot types

Support three snapshot levels:

| Snapshot type                   | Description                              | Priority |
| ------------------------------- | ---------------------------------------- | -------: |
| Disk snapshot                   | Disk state only                          |       P0 |
| Suspend snapshot                | Disk + VM running state                  |       P1 |
| Application-consistent snapshot | Guest agent freezes filesystem/app state |       P2 |

Fast Mode should optimize for suspend/resume.
Compatibility Mode should expose detailed snapshot chains.

The macOS dashboard should surface the same snapshot list and restore metadata
boundary as the CLI/API: snapshot name, kind, recorded runtime state, disk-chain
or suspend-image metadata when present, and the latest restore record. It must
label restore actions as metadata-boundary operations until real memory restore,
live VM restore, and application-consistent guest quiescing are implemented.

## 12.4 Backup and export

Add:

```text
export VM bundle
import VM bundle
clone VM
linked clone
compact disk
verify disk
repair metadata
```

The Rust CLI/API/socket path already provides portable VM bundle export/import
for `.vmbridge` directories and `.tar` archives. The macOS dashboard should
surface that same daemon boundary as metadata/file-copy status: source and
destination paths, archive format, import rename results, copied files, and
preserved manifest/metadata such as snapshots, port forwards, and approved
shared folders. It must not add live behavior to export/import: no VM start, QMP
connection, guest-tools attachment, live socket copy, or live guest state
migration is implied.

The current metadata repair boundary is conservative: the CLI/socket API can
recreate missing repairable metadata from the manifest and snapshot list, report
the bundle, timestamp, repaired/no-op status, and action list, and leave live
storage untouched. The macOS dashboard repair panel/action should surface the
same daemon `repair_metadata` result and must not claim to create disks or
replace corrupt JSON.

Long-term:

```text
incremental backup
encrypted backup
OCI-style VM image registry
team templates
```

---

# 13. Networking plan

## 13.1 Default networking

Fast Mode should default to:

```text
NAT
automatic DNS
automatic guest hostname
automatic port discovery
sleep/wake recovery
VPN-aware behavior where possible
```

The user should not have to understand bridges, tap devices, or route tables.

## 13.2 Developer features

Developer-friendly networking:

```text
bridgevm ssh myvm
bridgevm port add myvm 3000:3000
bridgevm open myvm 3000
myvm.bridgevm.local
automatic forwarded-port suggestions
```

Phase-0 dashboard work should treat `bridgevm open`/open-port as a
metadata/planning boundary first. The macOS dashboard can surface the derived
host URL or command for an existing forwarded port, but the wording must not
claim that BridgeVM opened a browser, connected to the service, changed live
networking, or started a VM.

Phase-0 `networkd` is also a public planning boundary. Its CLI should expose
JSON `NetworkPlan` output for higher-level runner smoke tests and concise
summary output for operator checks while validating modes, capabilities, and
port-forward inputs. These checks prove metadata planning only: they must not
start QEMU, launch Apple VZ, create host-only interfaces, attach bridged
networking, modify live forwards, or start a VM.

## 13.3 Network modes

| Mode                | Fast Mode | Compatibility Mode |
| ------------------- | --------: | -----------------: |
| NAT                 |       Yes |                Yes |
| Port forwarding     |       Yes |                Yes |
| Host-only           |        P1 |                Yes |
| Isolated VM network |        P1 |                Yes |
| Bridged network     |        P2 |                Yes |
| Advanced tap/bridge |        No |                Yes |
| Per-VM firewall     |        P2 |                 P2 |

---

# 14. Automatic Resource Manager

This is mandatory for Parallels-like lightness.

The user should not be forced to manually choose CPU and RAM. Advanced users can, but the default should be automatic.

## 14.1 Inputs

Resource manager monitors:

```text
host CPU type
performance cores / efficiency cores
host memory
memory pressure
battery state
thermal pressure
VM foreground/background state
guest OS
guest workload
display visibility
external monitor state
disk pressure
```

## 14.2 Modes

Expose simple user profiles:

| Profile       | Behavior                                             |
| ------------- | ---------------------------------------------------- |
| Automatic     | Default, balances host and guest                     |
| Battery Saver | Lower vCPU, lower display FPS, aggressive suspend    |
| Performance   | Higher CPU burst, higher display priority            |
| Developer     | Better disk/network behavior, stable port forwarding |
| Office        | Fast resume, smooth UI, low background usage         |

## 14.3 Runtime behavior

```text
VM foreground:
increase CPU burst
increase display FPS
increase input priority

VM background:
lower display FPS
lower CPU priority
pause unnecessary polling

Mac on battery:
limit vCPU burst
reduce background I/O
suggest suspend

Mac sleeping:
freeze VM safely
restore time/network/display after wake
```

## 14.4 Hard performance budget

The team should treat these as release gates:

```text
No busy polling in UI.
No constant display repaint when guest is idle.
No background VM consuming high CPU without explanation.
No uncontrolled memory growth in the UI process.
No hidden QEMU process left after VM shutdown.
No broken network after sleep/wake.
```

---

# 15. UI and UX plan

## 15.1 Main dashboard

VM cards:

```text
VM name
guest OS
mode badge: Fast / Compatibility
state: running / paused / stopped
CPU/RAM usage
disk usage
last snapshot
diagnostics / performance artifact metadata
export/import file-copy status
open-port plan / forwarded-service metadata
SSH plan metadata
quick actions
```

Mode badge examples:

```text
Fast Mode · Optimized
Fast Mode · Tools Missing
Compatibility Mode · QEMU
Compatibility Mode · Emulated CPU
```

## 15.2 VM creation wizard

Recommended flow:

```text
1. Choose OS
2. Choose installer or download image
3. BridgeVM recommends Fast or Compatibility Mode
4. Choose disk size
5. Choose integration options
6. Create VM
```

## 15.3 Mode choice UI

The mode selection screen:

```text
Recommended: Fast Mode

Fast Mode
Runs with the best speed, battery life, and Mac integration.
Recommended for this operating system.

Compatibility Mode
Use this only if you need advanced settings or legacy hardware.
It may be slower.
```

For unsupported guests:

```text
Fast Mode is not available for this operating system.
This VM will use Compatibility Mode.
```

## 15.4 VM window

VM window controls:

```text
Start
Pause
Resume
Suspend
Restart
Shutdown
Full screen
Fit to window
Install/repair BridgeVM Tools
Shared folders
USB devices
Snapshots
```

## 15.5 “Lightness” UI

Show users why the app feels light:

```text
Battery Saver active
Display throttled in background
VM suspended while inactive
BridgeVM Tools installed
Shared folders optimized
```

This turns hidden optimization into user trust.

---

# 16. CLI and developer workflow

BridgeVM should be a GUI app and a developer tool.

## 16.1 CLI examples

```bash
bridgevm list
bridgevm create ubuntu --mode fast --memory auto --disk 80G
bridgevm create windows11 --mode fast
bridgevm create legacy-win7 --mode compatibility --arch x86_64
bridgevm start dev
bridgevm suspend dev
bridgevm resume dev
bridgevm ssh dev
bridgevm port add dev 3000:3000
bridgevm share add dev Projects ~/Projects
bridgevm snapshot create dev before-upgrade
bridgevm snapshot restore dev before-upgrade
bridgevm export dev --output dev.vmbridge
bridgevm doctor
```

## 16.2 CLI principles

```text
Everything in the GUI should be reproducible from CLI.
Every VM should have a manifest.
Every manifest should be portable.
Every error should be diagnosable.
```

## 16.3 Local API

Use:

```text
gRPC over Unix domain socket
```

APIs:

```proto
VmService
  ListVms
  CreateVm
  StartVm
  SuspendVm
  ResumeVm
  StopVm
  DeleteVm(metadata_only)
  CreateSnapshot
  RestoreSnapshot
  StreamVmEvents

ModeService
  RecommendMode
  ExplainMode
  ConvertMode

AgentService
  GetGuestStatus
  SyncClipboard
  MountShare
  ListGuestApps
  LaunchGuestApp
```

---

# 17. VM manifest format

Every VM should have a readable manifest.

## 17.1 Fast Mode manifest

```yaml
schemaVersion: bridgevm.io/v1
name: windows-work
mode: fast

guest:
  os: windows
  version: "11"
  arch: arm64

backend:
  preferred: lightvm-windows-arm
  fallback: qemu-hvf-restricted

resources:
  profile: automatic
  memory: auto
  cpu: auto

display:
  renderer: metal
  framePolicy: adaptive
  retina: true

storage:
  primary:
    path: disks/root.qcow2
    size: 128GiB
    discard: true

network:
  mode: nat
  hostname: windows-work.bridgevm.local
  forwards:
    - host: 3000
      guest: 3000

integration:
  tools: required
  clipboard: true
  dragDrop: true
  dynamicResolution: true
  sharedFolders: true

security:
  sharedFolderApproval: required
  guestCommandExecution: false
  signedAgentUpdates: true
```

## 17.2 Compatibility Mode manifest

```yaml
schemaVersion: bridgevm.io/v1
name: legacy-windows
mode: compatibility

guest:
  os: windows
  version: "7"
  arch: x86_64

backend:
  engine: qemu
  accelerator:
    preferred: hvf
    fallback: tcg

machine:
  firmware: bios
  chipset: i440fx
  cpuModel: qemu64

resources:
  cpu:
    cores: 2
  memory:
    size: 4GiB

display:
  backend: spice

storage:
  primary:
    path: disks/root.qcow2
    format: qcow2
    bus: ide

network:
  mode: nat
  adapter: e1000

advanced:
  qemuArgs:
    - "-usb"
    - "-device"
    - "usb-tablet"
```

---

# 18. Repository structure

```text
bridgevm/
├── apps/
│   └── macos/
│       ├── BridgeVM.xcodeproj
│       └── Sources/
│
├── crates/
│   ├── bridgevm-core/
│   ├── bridgevm-cli/
│   ├── bridgevm-api/
│   ├── bridgevm-config/
│   ├── bridgevm-storage/
│   ├── bridgevm-network/
│   ├── bridgevm-resource-manager/
│   ├── bridgevm-agent-protocol/
│   ├── bridgevm-lightvm/
│   ├── bridgevm-fullvm/
│   ├── bridgevm-qemu/
│   ├── bridgevm-apple-vz/
│   └── bridgevm-hvf/
│
├── runners/
│   ├── lightvm-runner/
│   ├── fullvm-runner/
│   └── displayd/
│
├── agents/
│   ├── windows/
│   ├── linux/
│   └── macos-guest/
│
├── packaging/
│   ├── macos/
│   ├── homebrew/
│   ├── linux/
│   └── windows/
│
├── docs/
│   ├── architecture/
│   ├── security/
│   ├── guest-tools/
│   ├── fast-mode/
│   ├── compatibility-mode/
│   └── contributing/
│
├── tests/
│   ├── integration/
│   ├── performance/
│   ├── snapshot/
│   ├── sleep-wake/
│   └── guest-tools/
│
└── examples/
    ├── manifests/
    ├── cloud-init/
    └── templates/
```

---

# 19. Recommended technology stack

## 19.1 macOS app

| Layer        | Recommendation                   |
| ------------ | -------------------------------- |
| UI           | SwiftUI + AppKit where necessary |
| Core daemon  | Rust                             |
| CLI          | Rust + clap                      |
| API          | gRPC over Unix socket            |
| Config       | YAML + JSON Schema               |
| Logs         | structured tracing               |
| Display      | Metal + CoreAnimation            |
| Packaging    | signed and notarized DMG/PKG     |
| Distribution | GitHub Releases + Homebrew Cask  |

## 19.2 Guest tools

| Guest       | Recommendation                     |
| ----------- | ---------------------------------- |
| Linux       | Rust daemon + systemd service      |
| Windows     | Rust service + native Windows APIs |
| macOS guest | Swift/Objective-C helper later     |
| Protocol    | protobuf over vsock/virtio-serial  |
| Updates     | signed packages                    |

Current Phase 0 update handling is passive only: `signedAgentUpdates: true`
allows the `agent-update` capability and lets an authenticated guest report
`AgentUpdateAvailable` metadata, but BridgeVM only records and reports current
version, available version, URL, signature, and observed timestamp. It does not
download, install, execute, or auto-update guest tools.

## 19.3 Compatibility backend

| Platform | Backend            |
| -------- | ------------------ |
| macOS    | QEMU + HVF/TCG     |
| Linux    | QEMU + KVM/libvirt |
| Windows  | QEMU + WHPX        |

---

# 20. Development roadmap

## Phase 0: Technical proof of concept

**Duration: 1–2 months**

Goal:

```text
Prove that the core architecture works.
```

Deliverables:

```text
SwiftUI VM dashboard prototype
Rust daemon prototype
Apple Virtualization Linux Arm64 live boot proof (explicit opt-in/manual evidence)
QEMU/HVF Windows 11 Arm live boot proof (explicit opt-in/manual evidence)
basic VM manifest
basic CLI
metadata lifecycle start/stop plus live backend start/stop proof when opted in
NAT networking plan plus live networking proof when opted in
serial log metadata plus live serial proof when opted in
initial host-side measurements plus live guest performance proof when opted in
```

Success criteria:

```text
Ubuntu Arm64 boot on Apple Silicon is proven by explicit live evidence.
Windows 11 Arm installer or desktop reachability is proven by explicit live evidence.
Daemon can create, start, stop, and metadata-delete VM records without destroying bundles by default.
CLI can list VM records and record lifecycle state; live VM start requires explicit backend evidence.
```

Current evidence note:

```text
The default Phase 0 hardening lane is metadata/dry-run safe: it verifies
manifest, daemon, lifecycle, runner-readiness, QMP, diagnostics, performance,
guest-tools, and bundle-management boundaries without starting QEMU, Apple VZ,
or a real VM. The same safe lane now includes the aggregate readiness CLI
contract and the Apple VZ live opt-in default-skip boundary. Two of the three
Phase 0 live boot criteria are now met:

- Compatibility Mode QEMU/HVF Linux Arm64: a Debian arm64 genericcloud guest was
  booted under `qemu-system-aarch64 -machine virt -accel hvf` on Apple Silicon,
  and the captured serial-sentinel + QMP `running` evidence bundle was verified
  and recorded through `bridgevm readiness --record-live-evidence` (see
  docs/compatibility-mode/README.md, "Live boot evidence review").
- Fast Mode Apple Virtualization Linux Arm64: a Debian arm64 netboot
  kernel/initrd guest was launched through the ad-hoc-signed `AppleVzRunner`
  (with the `com.apple.security.virtualization` entitlement) and booted to the
  Debian installer over the hvc0 serial console; the serial-sentinel evidence
  bundle was verified by `tests/integration/verify-apple-vz-live-evidence.sh` and
  recorded through `bridgevm readiness --record-live-evidence` (see
  docs/fast-mode/README.md, "Live evidence review").

QEMU/HVF Windows 11 Arm installer reachability is now demonstrated by explicit
live evidence: the Windows 11 25H2 Arm64 ISO was booted under
`qemu-system-aarch64 -machine virt -accel hvf -cpu host -bios
edk2-aarch64-code.fd -device ramfb` (ISO via `usb-storage`, USB keyboard for the
"Press any key to boot" prompt) and reached the Windows Setup "Select language
settings" screen, captured via a QMP `screendump` viewer frame with QMP
`query-status` running. This satisfies the Phase 0 "Windows 11 Arm installer ...
reachability" criterion as evidence.

The product's own QEMU launch command now boots the Windows installer: a
`windows-installer` boot mode (`bridgevm-config`) plus installer-media wiring in
`build_compatibility_command` (`bridgevm-qemu`) attaches the `boot.installerImage`
ISO as a bootable USB CD-ROM with a `ramfb` GOP framebuffer and a USB HID stack.
`bridgevm run <vm> --spawn` for a compatibility windows/arm64 VM in this mode
launched Windows 11 25H2 Arm64 to the Setup language screen, confirmed by a QMP
screendump with `query-status` running (see
docs/compatibility-mode/README.md, "Windows 11 Arm installer boot").

One follow-up remains for Windows: the readiness evidence recorder cannot yet
ingest this graphical proof — `live_boot_progress_proven` is serial-sentinel-only
and Windows GUI Setup emits no serial console output, so formally recording
Windows boot progress needs an enhancement that accepts a verified graphical
viewer frame as boot-progress evidence for graphical-only guests.
```

Current scaffold progress:

```text
Rust CLI/API/daemon metadata lifecycle is in place.
Compatibility Mode can build QEMU command plans and supervise spawned backends.
Primary disk prepare/create/inspect metadata is implemented.
Active disk verify/compact metadata is implemented and surfaced in the macOS dashboard.
Metadata repair is implemented in the Rust CLI/API/daemon and surfaced in the macOS dashboard as a metadata-only repair/no-op action summary.
Manifest migration has a conservative CLI/API/daemon metadata boundary: current-schema manifests can be dry-run or no-op migrated with receipt/backup metadata, while future schemas and malformed YAML are rejected before writing receipts.
The macOS daemon client and dashboard now carry the same manifest migration request/response DTO boundary, including a dry-run dashboard check for backup, receipt, schema, and action metadata, without adding live bundle mutation outside the daemon contract.
Metadata-only delete is the dashboard-safe delete path: it refuses running VMs, preserves the .vmbridge bundle and manifest, writes deletion tombstone metadata, and hides deleted VMs from inventory.
Portable VM bundle export/import for .vmbridge directories and .tar archives is implemented in the Rust CLI/API/socket path and surfaced in the macOS dashboard as metadata/file-copy status.
Export/import smoke coverage verifies portable manifest/metadata preservation while excluding transient live `.sock` and `.lock` artifacts from directory exports, tar exports, and imported bundles.
Manifest port-forward list/add/remove is implemented in the Rust CLI/API/daemon and surfaced in the macOS dashboard as recorded networking policy only; open-port and SSH planning are surfaced as derived metadata/commands without executing `open` or `ssh`.
Disk snapshot metadata records qcow2 chain intent.
snapshot disk-create can materialize the recorded qcow2 overlay when the backing image exists.
Runner startup resolves the selected active disk chain member.
Disk snapshot restore rewinds active disk metadata to the selected snapshot backing image.
macOS dashboard snapshot list/restore/chain/disk-create UI surfaces the same metadata boundary.
Shared-folder manifest list/add/remove is implemented in the Rust CLI/API/daemon and surfaced in the macOS dashboard as approved-path policy only, with local/socket smoke coverage for invalid empty or whitespace share fields and duplicate policy entries.
Guest tools inline file-drop and application/window list/launch/focus/close command dispatch is implemented in CLI/API/socket paths and surfaced in the macOS dashboard as safe alpha command/result metadata, not as proof of real host-to-guest filesystem drop, mounted guest filesystem, or real desktop control.
Guest tools token/status, Linux command rendering, and authenticated `GuestHello` acceptance have CLI/socket smoke coverage for valid sessions, wrong-token rejection, disallowed-capability rejection, and token-value non-leakage through generated Linux tools argv. Guest tools time-sync socket dispatch has live fake-socket smoke coverage, command tracking has negative-path smoke coverage for duplicate pending IDs and stray `CommandResult` frames, and `AgentUpdateAvailable` has no-real-VM fake-socket smoke coverage as a protocol/capability metadata notice only, not as an auto-update installer, downloader, or executor.
QMP supervisor metadata has daemon fake-socket unit coverage for terminal-event cleanup, nonterminal event retention, and drain-limit metadata without starting QEMU.
Fast Mode template boot-media readiness and resource-profile launch-spec handoff have CLI/socket smoke coverage as metadata-only contract locks; they verify local media import/status, inert download planning, readiness blockers, and Apple VZ validate-only config-plan handoff without starting QEMU, Apple VZ, or a VM.
Aggregate readiness-report CLI coverage and the Apple VZ live opt-in default-skip smoke now lock the pre-launch report contract and default no-live-start boundary without treating either path as live E2E proof.
Windows 11 Arm restricted backend planning has CLI/socket smoke coverage for `qemu-args` selection of QEMU aarch64, `virt`, `hvf`, `cpu host`, restricted display defaults, explicit VNC preservation, and the restricted-profile RNG device without spawning QEMU or claiming that Windows booted.
Performance baseline/sample CLI/socket smoke coverage records metadata-only baselines and bounded host-side sample artifacts, with dashboard-facing artifact/card metadata, without booting, resuming, or benchmarking a guest.
Real suspend memory serialization/restoration now works at the Fast Mode runner level: AppleVzRunner implements Apple VZ `saveMachineState`/`restoreMachineState` (CLI `--save-state`/`--restore-state`), persisting the machine identifier and network MAC per bundle so the restore configuration matches the saved state. A real Debian arm64 VZ guest was suspended (memory+device state written to a file) and restored/resumed. This is now wired through the whole product: `bridgevm suspend <vm>` / `resume <vm>` (and the daemon `suspend_backend`/`resume_backend` requests) spawn `lightvm-runner` with the save/restore flags, record the saved state at `metadata/suspend-images/<vm>.bin`, and transition the VM to `suspended`/`running`; the macOS app's pause/resume actions send the same daemon requests. Verified end to end via `bridgevm suspend`/`resume` (98 MB state saved, then restored to a running guest). Remaining follow-ups: pausing an already-running VM via IPC (the current model boots the Fast VM, runs briefly, then saves), Compatibility Mode suspend/resume, daemon-supervised tracking of the resumed child, and guest-agent application consistency.
```

---

## Phase 1: Fast Mode Linux MVP

**Duration: 3–6 months**

Goal:

```text
Make the first truly lightweight daily-use VM experience.
```

Target:

```text
Apple Silicon Mac
Ubuntu/Fedora/Debian Arm64
Fast Mode only
```

Features:

```text
OS download/template flow
Apple VZ backend
VM creation wizard
NAT networking
disk creation
basic snapshots
SwiftUI console
Metal display prototype
BridgeVM Tools for Linux alpha
dynamic resolution
clipboard text sync
virtiofs shared folders
suspend/resume
bridgevm ssh
dashboard open-port plan metadata
dashboard shared-folder manifest policy UI
bridgevm doctor
```

Success criteria:

```text
A developer can use an Ubuntu Arm64 VM daily.
Idle CPU is low.
Resize feels smooth.
Shared folder works.
Clipboard works.
Suspend/resume is reliable.
```

Release:

```text
BridgeVM 0.1 Developer Preview
```

---

## Phase 2: Compatibility Mode MVP

**Duration: 3–4 months after Phase 1**

Goal:

```text
Add the expert engine without polluting Fast Mode.
```

Features:

```text
QEMU backend
HVF acceleration on macOS
TCG fallback
ISO boot
manual VM creation
advanced settings screen
qcow2 storage
SPICE/VNC fallback display
basic QMP integration
QEMU log viewer
QEMU args export
```

Success criteria:

```text
x86_64 Linux boots in Compatibility Mode.
Legacy ISO boot works.
Advanced users can inspect and modify hardware settings.
Fast Mode remains simple and unaffected.
```

Release:

```text
BridgeVM 0.2 Compatibility Preview
```

---

## Phase 3: Windows 11 Arm beta

**Duration: 6–12 months**

Goal:

```text
Make Windows 11 Arm usable for productivity.
```

Features:

```text
Windows 11 Arm setup wizard
UEFI and TPM configuration
restricted QEMU/HVF Windows profile
VirtIO driver guidance
BridgeVM Tools for Windows alpha
clipboard
dynamic resolution
shared folders prototype
drag-and-drop prototype
time sync
guest IP reporting
suspend/resume improvements
```

Important legal/product rule:

```text
Do not claim Microsoft-authorized Windows status.
Tell users they need a valid Windows license.
Be transparent about limitations.
```

Success criteria:

```text
Windows 11 Arm installs with guided flow.
Office/browser/productivity apps are usable.
Clipboard and resolution work.
VM resume is much faster than cold boot.
User clearly understands support limitations.
```

Release:

```text
BridgeVM 0.5 Windows Beta
```

---

## Phase 4: Parallels-like integration

**Duration: 12–24 months**

Goal:

```text
Move from “good VM app” to “Mac-integrated VM experience.”
```

Features:

```text
BridgeVM Tools stable
shared folders stable
drag-and-drop stable
app launcher
Dock integration prototype
file association
window metadata
pseudo-Coherence for Linux first
pseudo-Coherence for Windows later
Metal display pipeline v2
adaptive resource manager
battery mode
sleep/wake recovery
snapshot tree UI
clone VM
export/import
```

Success criteria:

```text
Users can launch guest apps from macOS.
Windows productivity apps feel integrated.
Linux desktop feels smooth and light.
VM idle usage is dramatically lower than Compatibility Mode.
```

Release:

```text
BridgeVM 1.0
```

---

## Phase 5: Custom Fast Mode VMM and graphics R&D

**Duration: 24–36+ months**

Goal:

```text
Reduce QEMU dependency for the high-value Fast Mode path.
```

Features:

```text
custom Hypervisor.framework VMM research
minimal virtio device stack
custom block device
custom network device
custom display device
Windows shared-folder driver/helper
WDDM/graphics research
Direct3D-to-Metal feasibility study
true Coherence research
enterprise image templates
team VM profiles
```

Success criteria:

```text
Windows 11 Arm Fast Mode no longer depends on full QEMU for core execution path, or QEMU is reduced to a replaceable backend.
Display latency and idle CPU approach commercial-grade behavior.
```

Release:

```text
BridgeVM 2.0 direction
```

---

# 21. MVP feature definition

The first public MVP should **not** try to beat Parallels for Windows. It should first beat UTM-style heaviness for Linux Arm64.

## BridgeVM 0.1 MVP

Must include:

```text
Fast Mode
Apple Silicon support
Ubuntu Arm64 VM creation
Fedora Arm64 VM creation
basic Debian Arm64 support
VM start/stop/suspend/resume
NAT networking
basic port forwarding
shared folder
clipboard text sync
dynamic resolution
Metal display prototype
CLI
VM manifest
logs
diagnostic bundle
```

Must not include:

```text
Coherence
Windows gaming
DirectX acceleration
full USB passthrough
x86_64 Fast Mode
enterprise management
plugin marketplace
cloud sync
```

The MVP promise should be:

> **The lightest open-source Linux Arm64 VM experience for Apple Silicon Macs.**

---

# 22. Fast Mode versus Compatibility Mode comparison

| Area                    | Fast Mode                    | Compatibility Mode        |
| ----------------------- | ---------------------------- | ------------------------- |
| Product goal            | Parallels-like lightness     | UTM/QEMU-like flexibility |
| Internal name           | `LightVM`                    | `FullVM`                  |
| Main backend            | Apple VZ / custom HVF path   | QEMU                      |
| User type               | normal users, developers     | experts, researchers      |
| Supported OS            | limited                      | broad                     |
| x86 emulation           | no                           | yes                       |
| Legacy OS               | no                           | yes                       |
| Advanced device config  | mostly hidden                | exposed                   |
| Display                 | Metal optimized              | SPICE/VNC/QEMU fallback   |
| Guest Tools             | required for best experience | optional/partial          |
| Resource management     | automatic                    | manual + automatic        |
| Battery optimization    | aggressive                   | limited                   |
| Coherence-like features | planned                      | limited                   |
| Error messages          | simple                       | detailed                  |
| QEMU args               | hidden                       | visible/editable          |

---

# 23. Security plan

## 23.1 Security principles

```text
VMs are not automatically trusted.
Guest Tools are privileged and must be restricted.
Shared folders are dangerous.
Clipboard sync can leak secrets.
USB passthrough must require explicit approval.
Every guest-host channel needs validation.
```

## 23.2 Host process separation

```text
BridgeVM.app
normal user UI

bridgevmd
core daemon, minimal privileges

lightvm-runner
one process per Fast Mode VM

fullvm-runner
one process per Compatibility Mode VM

displayd
display compositor

network-helper
only network privileges

storage-helper
disk/snapshot operations

agentd
guest tools protocol broker
```

## 23.3 Permission model

Per VM:

```yaml
permissions:
  clipboard:
    enabled: true
    directions: bidirectional
  sharedFolders:
    allowedPaths:
      - ~/Projects
    readonly: false
  usb:
    enabled: false
  guestToHostCommands:
    enabled: false
  hostToGuestCommands:
    enabled: false
```

## 23.4 Security features

```text
signed releases
signed guest tools
notarized macOS builds
sandbox where possible
QEMU process sandboxing where possible
path allowlists
read-only shared folders
clipboard auto-expire option
diagnostic logs without secrets
private security disclosure process
```

---

# 24. Testing strategy

## 24.1 Test categories

| Test type         | Purpose                                       |
| ----------------- | --------------------------------------------- |
| Unit tests        | config, API, mode selection                   |
| Integration tests | create/start/stop VM                          |
| Guest tests       | tools install, clipboard, resolution          |
| Display tests     | resize, scaling, cursor                       |
| Storage tests     | snapshot, restore, compaction                 |
| Network tests     | NAT, port forwarding, sleep/wake              |
| Performance tests | boot, resume, idle CPU, disk I/O              |
| Security tests    | shared folder boundaries, protocol validation |
| Upgrade tests     | manifest migration                            |
| Crash tests       | killed runner, killed daemon, host sleep      |

## 24.2 Test matrix

Initial matrix:

| Host                | Guest          | Mode              | Priority |
| ------------------- | -------------- | ----------------- | -------- |
| Apple Silicon macOS | Ubuntu Arm64   | Fast              | P0       |
| Apple Silicon macOS | Fedora Arm64   | Fast              | P0       |
| Apple Silicon macOS | Debian Arm64   | Fast              | P1       |
| Apple Silicon macOS | Windows 11 Arm | Fast experimental | P0/P1    |
| Apple Silicon macOS | Ubuntu x86_64  | Compatibility     | P1       |
| Apple Silicon macOS | Windows x86_64 | Compatibility     | P2       |
| Linux x86_64        | Ubuntu x86_64  | Compatibility     | P2       |
| Windows x86_64      | Ubuntu x86_64  | Compatibility     | P3       |

## 24.3 Release gates

No release if:

```text
VM cannot reliably suspend/resume.
Shared folder can escape approved path.
Clipboard sync crashes guest tools.
VM process remains after stop.
Fast Mode idle CPU is unexpectedly high.
Sleep/wake breaks networking.
VM manifest migration corrupts settings.
```

---

# 25. Performance targets

These are internal engineering goals.

## 25.1 App-level targets

```text
BridgeVM app launch:
under 1 second target after warm launch

Dashboard idle CPU:
near 0%

Dashboard memory:
small enough that users do not feel the UI is heavy

VM list refresh:
event-driven, not polling-heavy
```

## 25.2 Fast Mode VM targets

```text
Linux Arm64 cold boot:
competitive with commercial VM apps

Linux Arm64 resume:
under 3 seconds target

Windows 11 Arm resume:
under 5 seconds target

Visible guest idle CPU:
under 1–2% target

Paused VM CPU:
near 0%

Background VM display:
adaptive low FPS or no repaint

Clipboard latency:
under 100 ms target

Shared folder:
fast enough for developer workflows
```

## 25.3 Compatibility Mode targets

Compatibility Mode can be heavier, but it should be honest.

Show:

```text
Hardware acceleration active: yes/no
CPU emulation active: yes/no
Expected performance: high/medium/low
Battery impact: low/medium/high
```

---

# 26. Open-source strategy

## 26.1 License recommendation

Use a hybrid license strategy:

| Component            | Suggested license                    |
| -------------------- | ------------------------------------ |
| Core daemon          | MPL-2.0 or Apache-2.0                |
| CLI                  | Apache-2.0                           |
| Guest Tools          | Apache-2.0 or MPL-2.0                |
| macOS app            | MPL-2.0                              |
| Docs                 | CC BY 4.0                            |
| QEMU changes, if any | Must follow QEMU license constraints |

For maximum enterprise adoption, use:

```text
Apache-2.0 for libraries and CLI
MPL-2.0 for app/core
```

For stronger copyleft protection, use:

```text
GPLv3 for app/core
Apache-2.0 for protocol libraries
```

Avoid modifying QEMU early. Prefer controlling QEMU as an external process through QMP/CLI. This reduces maintenance burden.

## 26.2 Governance

Start with:

```text
founder-led governance
maintainer council after 1.0
public RFC process
security disclosure policy
DCO sign-off
transparent roadmap
```

## 26.3 Community contribution areas

```text
OS templates
Linux distro profiles
QEMU compatibility profiles
documentation
translations
test results
guest tools fixes
performance benchmarks
packaging
```

Core team should keep control over:

```text
Fast Mode architecture
guest-host protocol
security model
display pipeline
Windows tools
resource manager
```

---

# 27. Team plan

## 27.1 Minimum serious team

| Role                    | Count | Responsibility                 |
| ----------------------- | ----: | ------------------------------ |
| Founder/architect       |     1 | product, architecture, roadmap |
| macOS engineer          |     1 | SwiftUI/AppKit, Apple APIs     |
| Rust systems engineer   |     1 | daemon, storage, networking    |
| virtualization engineer |     1 | QEMU/HVF/VZ, VM lifecycle      |
| guest tools engineer    |     1 | Linux/Windows tools            |
| QA/release engineer     |     1 | automated VM tests, packaging  |
| designer/docs           | 0.5–1 | UX, docs, onboarding           |

Minimum practical team:

```text
5–7 people for a serious MVP.
```

## 27.2 Strong team for Parallels-like quality

| Area                   | Count |
| ---------------------- | ----: |
| macOS native app       |     2 |
| Rust core/platform     |     3 |
| virtualization backend |     2 |
| Windows guest tools    |     2 |
| Linux guest tools      |     1 |
| display/graphics       |     2 |
| storage/network        |   1–2 |
| QA automation          |     2 |
| security               |     1 |
| product/design/docs    |     2 |

Realistic long-term team:

```text
15–20 people for commercial-grade quality.
```

---

# 28. Risk analysis

| Risk                                        |  Severity | Probability | Mitigation                                          |
| ------------------------------------------- | --------: | ----------: | --------------------------------------------------- |
| Windows graphics is too hard                | Very high |        High | Focus first on productivity apps and 2D UX          |
| Microsoft authorization issue               |      High |        High | Do not claim official Windows authorization         |
| Fast Mode becomes QEMU wrapper              |      High |      Medium | Keep separate LightVM architecture                  |
| Guest Tools complexity                      | Very high |        High | Build tools early, keep protocol simple             |
| Coherence takes too long                    |      High |        High | Build app launcher and pseudo-Coherence first       |
| Shared folders are slow                     |      High |      Medium | Use virtiofs/Linux first, custom Windows path later |
| Sleep/wake instability                      |      High |        High | Make sleep/wake a release gate                      |
| Security issue in guest-host channel        | Very high |      Medium | Capability tokens, validation, signed tools         |
| Too many OS targets                         |      High |        High | Apple Silicon + Arm64 first                         |
| Community focuses only on QEMU mode         |    Medium |      Medium | Clear governance around Fast Mode                   |
| App feels heavy despite good VM performance |      High |      Medium | Native UI, event-driven design, Metal display       |
| Licensing complexity                        |    Medium |      Medium | Avoid QEMU modification early                       |

---

# 29. Non-goals

The project should explicitly reject certain goals in the first two years.

Do not target early:

```text
perfect Windows gaming
DirectX 12 parity with Parallels
high-end CAD
GPU passthrough on Mac
all OSes in Fast Mode
x86_64 Fast Mode on Apple Silicon
full enterprise management
cloud sync
mobile/iPad support
kernel-level hypervisor from scratch
```

The first product should win a smaller battle:

```text
Apple Silicon Mac
Fast Linux Arm64
usable Windows 11 Arm productivity
excellent suspend/resume
excellent shared folders
excellent clipboard
excellent display smoothness
```

---

# 30. Final 12-month execution plan

## Months 1–2

```text
Architecture finalized
SwiftUI shell
Rust daemon
Apple VZ Linux boot
QEMU/HVF Windows Arm boot
manifest format
basic CLI
performance baseline
```

## Months 3–4

```text
Fast Mode Linux VM creation
Ubuntu/Fedora templates
NAT networking
disk manager
VM console
BridgeVM Tools Linux alpha
dynamic resolution alpha
clipboard alpha
```

## Months 5–6

```text
Metal display prototype
virtiofs shared folder
suspend/resume
bridgevm ssh
snapshot disk-only
diagnostic bundle
Homebrew Cask
BridgeVM 0.1 Developer Preview
```

## Months 7–8

```text
Compatibility Mode QEMU backend
ISO boot
advanced settings
QMP integration
SPICE/VNC fallback
x86 Linux compatibility VM
BridgeVM 0.2 Compatibility Preview
```

## Months 9–10

```text
Windows 11 Arm wizard
UEFI/TPM profile
Windows guest tools alpha
clipboard
dynamic resolution
time sync
guest IP reporting
restricted QEMU/HVF Fast Mode profile
```

## Months 11–12

```text
Windows shared folder prototype
drag-and-drop alpha
resource manager v1
battery mode
sleep/wake recovery tests
snapshot UI
signed releases
BridgeVM 0.5 Windows Beta
```

---

# 31. Final product rulebook

These rules should guide every engineering decision.

## Rule 1

**Fast Mode must stay narrow.**

If a feature makes Fast Mode heavier for everyone, move it to Compatibility Mode.

## Rule 2

**Compatibility Mode can be complex. Fast Mode cannot.**

Advanced users can handle complexity. Normal users should not see it.

## Rule 3

**Guest Tools are not optional for product quality.**

Without BridgeVM Tools, the product will feel like a generic VM viewer.

## Rule 4

**Display smoothness matters as much as CPU speed.**

Mouse latency, resize behavior, scrolling, and Retina scaling will define user perception.

## Rule 5

**Suspend/resume is more important than cold boot.**

Users care about how fast the VM comes back when they need it.

## Rule 6

**Do not overpromise Windows graphics.**

Win on productivity first. Treat DirectX/3D as long-term R&D.

## Rule 7

**Use QEMU for compatibility, not product identity.**

QEMU is a powerful backend, but BridgeVM should not become a QEMU settings panel.

## Rule 8

**Make performance visible.**

Show users whether hardware acceleration is active, whether guest tools are installed, and why a VM may be slow.

## Rule 9

**Everything should be scriptable.**

The GUI wins normal users. The CLI/API wins developers.

## Rule 10

**The app must feel native before it feels powerful.**

No Electron first version. No heavy dashboard. No unnecessary background services.

---

# 32. Final recommendation

Build BridgeVM as a **two-mode open-source Mac virtualization product**:

```text
Fast Mode:
Parallels-like
Apple Silicon first
Arm64 first
native UI
Metal display
BridgeVM Tools
automatic resources
low idle CPU
fast suspend/resume

Compatibility Mode:
UTM/QEMU-like
wide OS support
legacy support
x86 emulation
advanced settings
QEMU backend
expert workflows
```

The final strategic sentence is:

> **BridgeVM should not try to make every VM fast. It should make supported VMs extremely fast, and unsupported VMs possible.**

[1]: https://www.qemu.org/docs/master/system/introduction.html?utm_source=chatgpt.com "Introduction — QEMU documentation"
[2]: https://developer.apple.com/documentation/virtualization?utm_source=chatgpt.com "Virtualization | Apple Developer Documentation"
[3]: https://developer.apple.com/documentation/hypervisor?utm_source=chatgpt.com "Hypervisor | Apple Developer Documentation"
[4]: https://www.qemu.org/docs/master/system/whpx.html?utm_source=chatgpt.com "Windows Hypervisor Platform"
[5]: https://libvirt.org/?utm_source=chatgpt.com "libvirt: The virtualization API"
[6]: https://support.microsoft.com/en-us/windows/options-for-using-windows-11-with-mac-computers-with-apple-m1-m2-and-m3-chips-cd15fd62-9b34-4b78-b0bc-121baa3c568c?utm_source=chatgpt.com "Options for using Windows 11 with Mac® computers ..."
[7]: https://docs.parallels.com/landing/pdfm-ug/v19-en-us/parallels-desktop-for-mac-19-users-guide/advanced-topics/installing-and-updating-parallels-tools/parallels-tools-overview?utm_source=chatgpt.com "Parallels Tools Overview | User's Guide v19"
[8]: https://kb.parallels.com/en/4670?utm_source=chatgpt.com "What is Coherence? Information about Coherence View ..."
[9]: https://mac.getutm.app/?utm_source=chatgpt.com "UTM | Virtual machines for Mac"
[10]: https://libvirt.org/kbase/virtiofs.html?utm_source=chatgpt.com "Sharing files with Virtiofs"
