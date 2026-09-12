#!/usr/bin/env python3
"""Synthetic asset identity tests, not installed Windows boot evidence."""
import hashlib
import json
from pathlib import Path
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "scripts/live-gates"))
import guest_input_live_inputs as inputs


class InputContracts(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name).resolve()
        firmware = self.root / "crates/bridgevm-hvf/firmware/edk2-aarch64-secure-code.fd"
        firmware.parent.mkdir(parents=True)
        self.paths = {"image": self.root / "image", "vars": self.root / "vars",
                      "binary": self.root / "binary", "firmware": firmware}
        for name, path in self.paths.items():
            with path.open("wb") as stream:
                stream.truncate(64 * 1024 * 1024 if name == "vars" else 512)
        self.value = {"schema": "bridgevm.guest-input-live.v1", "purpose": "diagnostic-only",
                      "profile": inputs.PROFILE,
                      "assets": {name: {"path": str(path), "sha256": inputs.digest(path)}
                                 for name, path in self.paths.items()}}
        self.manifest = self.root / "manifest.json"

    def load(self):
        self.manifest.write_text(json.dumps(self.value))
        return inputs.load(self.manifest, self.root)

    def test_valid_seal(self):
        paths, hashes, manifest_hash = self.load()
        self.assertEqual(paths, self.paths)
        self.assertEqual(manifest_hash, hashlib.sha256(self.manifest.read_bytes()).hexdigest())
        self.assertEqual(hashes["image"], self.value["assets"]["image"]["sha256"])

    def test_hash_mutation_refused(self):
        self.paths["image"].write_bytes(b"1" * 512)
        with self.assertRaises(ValueError):
            self.load()

    def test_wrong_slot_refused(self):
        self.paths["vars"].write_bytes(b"0" * 65536)
        self.value["assets"]["vars"]["sha256"] = inputs.digest(self.paths["vars"])
        with self.assertRaises(ValueError):
            self.load()

    def test_symlink_refused(self):
        link = self.root / "alias"
        link.symlink_to(self.paths["binary"])
        self.value["assets"]["binary"]["path"] = str(link)
        with self.assertRaises(ValueError):
            self.load()

    def test_alias_and_claim_profile_refused(self):
        self.value["assets"]["binary"] = dict(self.value["assets"]["image"])
        with self.assertRaises(ValueError):
            self.load()
        self.value["purpose"] = "release"
        with self.assertRaises(ValueError):
            self.load()


if __name__ == "__main__":
    unittest.main()
