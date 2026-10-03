source "$ROOT/scripts/run-hvf-windows-installed-boot-usage-core.sh"

usage() {
  installed_boot_core_usage
  cat >&2 <<'EOF'
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
                          and a 100 ms CPU evidence/fallback readback cadence.
                          The recorded, media-independent mode can return to
                          balanced without changing VM media.
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
