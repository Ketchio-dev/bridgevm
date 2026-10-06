#!/usr/bin/env python3
"""Exercise pin loading and guard builder, runner and live-smoke agreement."""
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
PIN = "crates/bridgevm-hvf/firmware/bridgevm-pc-dxe-entry.sha256"
READER = "scripts/read-bridgevm-pc-firmware-pin.py"


def drift_errors(sources):
    errors = []
    for name, source in sources.items():
        if re.search(r'(?:EXPECTED_FD_SHA256\s*[:=]|firmware_sha256=)[^\n]*[0-9a-f]{64}', source):
            errors.append(name + ": embeds a separate firmware digest")
        if name == "runner":
            if '/firmware/bridgevm-pc-dxe-entry.sha256' not in source:
                errors.append(name + ": does not embed the approved pin")
        elif PIN not in source or READER not in source:
            errors.append(name + ": does not use the strict pin reader")
    return errors


class FirmwarePins(unittest.TestCase):
    def test_reader_matches_the_committed_pin(self):
        result = subprocess.run([sys.executable, ROOT / READER, ROOT / PIN],
                                capture_output=True, check=True)
        self.assertEqual(result.stdout, (ROOT / PIN).read_bytes())

    def test_reader_rejects_missing_and_malformed_inputs(self):
        with tempfile.TemporaryDirectory(prefix="bridgevm-pin-") as directory:
            path = Path(directory) / "pin"
            for raw in [None, b"", b"a" * 64, b"A" * 64 + b"\n", b"g" * 64 + b"\n",
                        b"a" * 64 + b"\n\n", b"a" * 63 + b"\n", b"a" * 65 + b"\n"]:
                if raw is not None:
                    path.write_bytes(raw)
                result = subprocess.run([sys.executable, ROOT / READER, path], capture_output=True)
                self.assertNotEqual(result.returncode, 0, raw)
                self.assertEqual(result.stdout, b"", raw)

    def test_production_consumers_and_drift_mutations(self):
        paths = {
            "builder": "scripts/build-bridgevm-pc-dxe-entry-firmware.sh",
            "smoke": "tests/integration/bridgevm-pc-dxe-entry-live-opt-in-smoke.sh",
            "runner": "crates/bridgevm-hvf/examples/bridgevm_pc_dxe_entry_live/contract/approved_firmware.rs",
        }
        sources = {name: (ROOT / path).read_text() for name, path in paths.items()}
        self.assertEqual(drift_errors(sources), [])
        for name in sources:
            mutated = dict(sources)
            mutated[name] = sources[name].replace("bridgevm-pc-dxe-entry.sha256", "other.sha256")
            self.assertTrue(drift_errors(mutated), name)
        for name, prefix in [("builder", 'EXPECTED_FD_SHA256="'), ("smoke", "firmware_sha256=")]:
            mutated = dict(sources)
            mutated[name] += "\n" + prefix + "a" * 64 + "\n"
            self.assertTrue(drift_errors(mutated), name)


if __name__ == "__main__":
    unittest.main()
