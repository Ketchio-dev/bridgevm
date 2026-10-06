installed_boot_core_usage() {
  cat >&2 <<'EOF'
usage: scripts/run-hvf-windows-installed-boot.sh --target RAW --vars FD --evidence-dir DIR [options]

Required:
  --target RAW            Installed Windows raw disk to boot.
  --vars FD               Writable UEFI vars file preserved from install.
  --firmware-code FD      AArch64 EDK2 code volume up to 64 MiB. Defaults to
                          BridgeVM's pinned secure+TPM2 firmware, then legacy
                          bundled or standard QEMU firmware.
  --evidence-dir DIR      Directory for preflight.txt, run.log, target-stat.txt, cleanup.txt, ramfb/.

Options:
  --placeholder-nsid1 RAW Blank NSID-1 disk; when set, target boots as NSID-2.
  --watchdog-ms N         Probe watchdog in milliseconds. Default: 900000.
  --enable-el2            A1 experiment: create the VM with EL2 enabled so
                          guest interrupt virtualization runs through HVF's
                          vEL2 path instead of the EL1-only GICv3 emulation.
  --exit-on-reset         Product reset mode: a guest SYSTEM_RESET exits the
                          probe with code 42 for the supervisor instead of
                          rebooting in process (PLAN.md R1).
  --no-watchdog           Keep the VM running until guest/user shutdown. This
                          is the normal app mode and cannot be combined with
                          --watchdog-ms. Agent overdue telemetry remains active.
  --max-reboots N         Maximum PSCI SYSTEM_RESET reboots. Default: 8.
  --max-exits N           Per-vCPU HVF exit cap. Default: 50000000.
  --ram-mib N             Guest RAM in MiB. Default: 4096.
  --smp-cpus N            Guest vCPU count, 1..123. Default: the probe's smp=1.
  --diagnostic-output-bounds  D4 only: no disk harvest, agent, vTPM, GPU or boot timer.
                          Bound frames/logs; preserve full bytes or refuse overflow.
  --ramfb-samples LIST    Comma-separated RAMFB sample ms values. Default:
                          1000,5000,15000,30000,60000,90000,120000.
  --display-export-ppm P  Atomically replace P with the current display frame.
  --display-export-ms N   Live display export interval, 100-60000 ms (default 500).
  --ramfb-display-export-ms N  3D-off (ramfb) --display-export-fb period, 16-60000 ms (default: as above).
  --input-control P       Read live KEY/POINTER/RESIZE/SNAPSHOT commands
                          appended to P. `SNAPSHOT label` writes a bounded
                          RAMFB/virtio-gpu checkpoint into the evidence dir.
  --boot-timer            Enable BOOT_TIMER milestone/ramfb/exits-per-sec logs
                          from hvf_gic_boot_probe.
  --boot-timer-ramfb-ms N Sample display checksums every N milliseconds for
                          BOOT_TIMER. Range: 100..60000. Implies
                          --boot-timer.
  --boot-timer-desktop-checksum64 N
                          Desktop checksum64 target as decimal or 0x-prefixed
                          hex. When matched, BOOT_TIMER reports desktop_reached.
                          Implies --boot-timer.
  --boot-timer-desktop-agent
                          Use the resident Windows logon agent READY/PONG as
                          the desktop oracle. This is stable across clock and
                          notification pixel changes. Implies --boot-timer.
  --shutdown-after-agent-ready
                          After the resident agent handshake, send the fixed
                          `shutdown.exe /p /f` command and require a guest
                          PSCI SYSTEM_OFF. This enables virtio-console and is
                          intended for clean, repeatable evidence runs. Agent
                          polling uses a periodic host wake so it does not add
                          an every-vCPU-exit automation lock to boot timing.
  --host-pause-resume-proof-ms N
                          After the agent service is ready, stop the complete
                          probe process for N ms (100..60000), continue it,
                          require a post-resume agent command round trip, then
                          request clean guest shutdown. The gate proves only
                          process-resident host pause/resume; it is not a
                          disk-backed suspend image.
  --agent-service-control PATH
                          Keep the resident Windows agent channel active and
                          tail PATH for app-injected commands. This is the
                          explicit, audited app-service boundary; inherited
                          BRIDGEVM_* variables remain ignored.
  --agent-service-command COMMAND
                          Initial command before entering service mode
                          (default: whoami; no CR, LF, or |).
  --agent-clipboard-sync  Enable bidirectional macOS/guest clipboard sync in
                          agent service mode.
  --agent-share-host DIR  Host directory for bidirectional agent sharing.
  --agent-share-guest DIR Guest directory paired with --agent-share-host.
  --agent-share-ms N      Share scan interval, 500..60000 (default: 2000).
  --agent-share-max-kb N  Largest file synchronized in KiB, 1..1048576
                          (default: 8192). Use at least 32768 for the staged
                          viogpu3d render package.
  --no-guest-disk-harvest Skip raw-disk log mounts; incompatible with title gates.
  --enable-xhci           Leave xHCI present for desktop input diagnosis.
  --virtio-net            Attach the virtio-net NIC (BRIDGEVM_VIRTIO_NET=1)
                          with the userspace NAT backend.
  --hda                   Attach the Intel HDA audio device (BRIDGEVM_HDA=1);
                          Windows binds its in-box hdaudio driver.
  --hda-coreaudio         --hda plus real-time CoreAudio playback, so the guest
                          audio comes out the Mac speakers.
  --nvme-buffered-io      Force the byte-identical buffered NVMe data path for
                          an audited storage-integrity A/B diagnostic run.
                          The production default remains direct DMA.
EOF
}
