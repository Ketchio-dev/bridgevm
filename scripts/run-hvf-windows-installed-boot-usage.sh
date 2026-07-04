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
  --max-reboots N         Maximum PSCI SYSTEM_RESET reboots. Default: 8.
  --ram-mib N             Guest RAM in MiB. Default: 4096.
  --ramfb-samples LIST    Comma-separated RAMFB sample ms values. Default:
                          1000,5000,15000,30000,60000,90000,120000.
  --enable-xhci           Leave xHCI present for desktop input diagnosis.
  --setup-input-actions LIST
                          Optional xHCI setup-input keys: tab, enter, space,
                          win+r, lgui+r, text:<lowercase-alnum>.
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
  --skip-build            Reuse target/debug/examples/hvf_gic_boot_probe.
  --print-policy          Print the enforced policy and exit.
  -h, --help              Show this help.

Policy:
  The script launches with BRIDGEVM_DISABLE_XHCI=1 by default and a writable
  installed target so the installed OS can persist first-boot writes.
  Use --enable-xhci only for Workstream D desktop input diagnosis.
  With --placeholder-nsid1, the placeholder is NSID-1 and the target is writable NSID-2.
EOF
}
