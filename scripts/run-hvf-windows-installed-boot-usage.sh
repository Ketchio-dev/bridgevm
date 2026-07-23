usage() {
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
  --no-watchdog           Keep the VM running until guest/user shutdown. This
                          is the normal app mode and cannot be combined with
                          --watchdog-ms. Agent overdue telemetry remains active.
  --max-reboots N         Maximum PSCI SYSTEM_RESET reboots. Default: 8.
  --max-exits N           Per-vCPU HVF exit cap. Default: 50000000.
  --ram-mib N             Guest RAM in MiB. Default: 4096.
  --smp-cpus N            Guest vCPU count, 1..123. Default: unset, so the
                          probe uses its smp=1 fallback.
  --ramfb-samples LIST    Comma-separated RAMFB sample ms values. Default:
                          1000,5000,15000,30000,60000,90000,120000.
  --display-export-ppm P  Atomically replace P with the current display frame.
  --display-export-ms N   Live display export interval, 100-60000 ms (default 500).
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
  --vtpm-state-dir DIR    Enable the TPM 2.0 TIS/PPI device and preserve its
                          swtpm state in DIR. The launcher owns exactly one
                          swtpm process and fails closed if its sockets do not
                          become ready.
  --swtpm-bin CMD         swtpm command or executable path. Requires
                          --vtpm-state-dir; default: swtpm from PATH.
  --swtpm-key-stdin       Read exactly one 32-byte state key from standard
                          input and pass it to swtpm by inherited FD. Enables
                          AES-256-CBC encrypt-then-MAC state protection; the
                          key is never accepted as an argument or disk path.
  --performance-risk MODE Select balanced or aggressive (default: balanced).
                          Aggressive requires --virtio-gpu-3d and enables the
                          direct renderer, deferred scanout, IOSurface GPU blit,
                          and uncapped scanout readback. The mode is recorded in
                          preflight evidence and is media-independent, so a
                          failed run can return to balanced immediately.
  --virtio-gpu-3d         Attach the virtio-gpu PCI device with the selected
                          3D backend, expose the viogpu3d bind-id alias
                          DEV_10F7 by default, build hvf_gic_boot_probe with
                          --features venus, and enable JSONL GPU tracing.
  --virtio-gpu-device-id ID
                          Override the virtio-gpu PCI device id for
                          --virtio-gpu-3d. Supported IDs: 1050, 10f7.
                          Use 1050 for PR #943-style VirGL viogpu3d packages.
  --gpu-trace PATH        JSONL trace path for --virtio-gpu-3d. Default:
                          <evidence-dir>/virtio-gpu.jsonl.
  --gpu-trace-protocol P  Trace gate protocol: auto, venus, or virgl.
                          Default: auto. Explicit virgl also selects the
                          CGL-backed VirGL host runtime.
  --gpu-trace-submit-prefix N
                          Bytes of each SUBMIT_3D payload preserved in the
                          JSONL trace, 1..1048576 (default 32). Raise to
                          capture whole command streams for offline decoding.
  --require-gpu-trace-gate
                          After boot, run bridgevm hvf virtio-gpu-trace-report
                          with --require-p3-gate and fail the script if the P3
                          GPU trace gate fails.
  --viogpu3d-dir DIR      Optional test-signed viogpu3d package directory. When
                          present, require its UMD-registered render-candidate
                          classification before boot and write
                          p3-gpu-readiness.txt.
  --require-viogpu3d-readiness
                          Require a viogpu3d render candidate and a passing
                          readiness check before booting. Requires
                          --virtio-gpu-3d.
  --require-real-title-gate
                          Require a clean single-generation viogpu3d state,
                          PPSSPP alive on vulkan_virtio.dll for 30 seconds,
                          and at least 300 RESOURCE_FLUSH commands. Compatibility
                          alias for the bundled PPSSPP --title-manifest plus
                          --require-title-gates.
  --title-manifest PATH   Repeatable version-1 JSON title contract describing
                          the expected guest log, pass marker, runtime, loaded
                          module, window, executable hash, and GPU flush floor.
                          Requires --virtio-gpu-3d.
  --require-title-gates   Fail the run unless every supplied title manifest
                          passes with a fresh guest log and clean driver state.
  --daily                 Opt-in daily-driver preset. Changes defaults only
                          when not explicitly overridden: --ram-mib 6144 and
                          --watchdog-ms 86400000 unless --no-watchdog is set.
                          Also sets --smp-cpus 4
                          unless --smp-cpus is supplied, pins xHCI report
                          pacing at 30ms, and implies --release unless
                          --skip-build is set.
  --setup-input-actions LIST
                          Optional comma-separated xHCI setup-input keys:
                          tab, enter, space, esc, backspace, delete,
                          f1..f12, arrows, home/end, pageup/pagedown,
                          ctrl+alt+delete,
                          win+r, lgui+r, text:<printable ASCII except comma>.
                          Requires --enable-xhci.
  --setup-input-marker TEXT
                          Serial marker that arms setup-input. Default is
                          the probe default when actions are set.
  --setup-input-fire-delay-ms N
                          Delay after marker before setup-input fires. Default: 0.
  --setup-input-ramfb-delay-ms LIST
                          Comma-separated RAMFB checkpoints after setup-input.
  --setup-input2-actions LIST
                          Optional second xHCI setup-input action sequence using
                          the same token grammar. Requires --enable-xhci.
  --setup-input2-marker TEXT
                          Serial marker that arms the second setup-input.
  --setup-input2-fire-delay-ms N
                          Delay after marker before the second setup-input fires.
  --setup-input2-ramfb-delay-ms LIST
                          RAMFB checkpoints after the second setup-input.
  --setup-input3-actions LIST
                          Optional third xHCI setup-input action sequence using
                          the same token grammar. Requires --enable-xhci.
  --setup-input3-marker TEXT
                          Serial marker that arms the third setup-input.
  --setup-input3-fire-delay-ms N
                          Delay after marker before the third setup-input fires.
  --setup-input3-ramfb-delay-ms LIST
                          RAMFB checkpoints after the third setup-input.
  --pointer-input-actions LIST
                          Optional xHCI absolute pointer actions:
                          move/press/release/click:<x>x<y>, right-click:<x>x<y>,
                          scroll:<-127..127>@<x>x<y>; center is also accepted.
                          Coordinates are decimal 0..32767. Requires --enable-xhci.
  --pointer-input-marker TEXT
                          Serial marker that arms pointer-input. Default is
                          the probe default when actions are set.
  --pointer-input-fire-delay-ms N
                          Delay after marker before pointer-input fires. Default: 0.
  --pointer-input-ramfb-delay-ms LIST
                          RAMFB checkpoints after pointer-input.
  --release               Build and run target/release/examples/hvf_gic_boot_probe.
  --skip-build            Reuse the selected profile's existing hvf_gic_boot_probe.
  --print-policy          Print the enforced policy and exit.
  -h, --help              Show this help.

Policy:
  The script launches with BRIDGEVM_DISABLE_XHCI=1 by default and a writable
  installed target so the installed OS can persist first-boot writes.
  Use --enable-xhci only for Workstream D desktop input diagnosis.
  With --placeholder-nsid1, the placeholder is NSID-1 and the target is writable NSID-2.
EOF
}
