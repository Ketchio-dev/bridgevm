usage() {
  cat >&2 <<'EOF'
usage: scripts/run-hvf-windows-installed-boot.sh --target RAW --vars FD --evidence-dir DIR [options]

Required:
  --target RAW            Installed Windows raw disk to boot.
  --vars FD               Writable UEFI vars file preserved from install.
  --evidence-dir DIR      Directory for preflight.txt, run.log, target-stat.txt, cleanup.txt, ramfb/.

Options:
  --placeholder-nsid1 RAW Blank NSID-1 disk; when set, target boots as NSID-2.
  --watchdog-ms N         Probe watchdog in milliseconds. Default: 900000.
  --no-watchdog           Keep the VM running until guest/user shutdown. This
                          is the normal app mode and cannot be combined with
                          --watchdog-ms. Agent overdue telemetry remains active.
  --max-reboots N         Maximum PSCI SYSTEM_RESET reboots. Default: 8.
  --ram-mib N             Guest RAM in MiB. Default: 4096.
  --smp-cpus N            Guest vCPU count, 1..123. Default: unset, so the
                          probe uses its smp=1 fallback.
  --ramfb-samples LIST    Comma-separated RAMFB sample ms values. Default:
                          1000,5000,15000,30000,60000,90000,120000.
  --display-export-ppm P  Atomically replace P with the current display frame.
  --display-export-ms N   Live display export interval, 100-60000 ms (default 500).
  --input-control P       Read live KEY/POINTER commands appended to P.
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
  --nvme-buffered-io      Force the byte-identical buffered NVMe data path for
                          an audited storage-integrity A/B diagnostic run.
                          The production default remains direct DMA.
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
  --daily                 Opt-in daily-driver preset. Changes defaults only
                          when not explicitly overridden: --ram-mib 6144 and
                          --watchdog-ms 86400000 unless --no-watchdog is set.
                          Also sets --smp-cpus 4
                          unless --smp-cpus is supplied, pins xHCI report
                          pacing at 30ms, and implies --release unless
                          --skip-build is set.
  --setup-input-actions LIST
                          Optional comma-separated xHCI setup-input keys:
                          tab, enter, space,
                          win+r, lgui+r, text:<[a-z0-9/.-]+>.
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
                          move:<x>x<y>, click:<x>x<y>, click:center.
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
