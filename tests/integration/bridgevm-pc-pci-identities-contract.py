#!/usr/bin/env python3
"""Compile and exercise the actual DXE PCI identity producer without a VM."""
from pathlib import Path
import os
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[2]
PACKAGE = ROOT / "crates/bridgevm-hvf/firmware/BridgeVmPcPkg"
FIXTURE = Path(__file__).with_name("bridgevm_pc_pci_identity_fixture")
with tempfile.TemporaryDirectory(prefix="bridgevm-pci-identities-") as directory:
    binary = Path(directory) / "identities"
    source = PACKAGE / "Library/PciProbeLib/PciIdentities.c"
    subprocess.run([
        os.environ.get("CC", "cc"), "-std=c11", "-Wall", "-Wextra", "-Werror",
        "-I", str(FIXTURE), "-I", str(PACKAGE / "Include"),
        f'-DBRIDGEVM_PCI_IDENTITIES_SOURCE="{source}"',
        str(FIXTURE / "main.c"), "-o", str(binary),
    ], check=True)
    raise SystemExit(subprocess.run([str(binary)], check=False).returncode)
