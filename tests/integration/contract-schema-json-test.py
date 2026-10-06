#!/usr/bin/env python3
"""Owned malformed module fixtures for the split deviation registry."""

import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location("contract_json", ROOT / "scripts/check-contract-schema-json.py")
CHECK = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECK)
sys.path.insert(0, str(ROOT / "scripts"))
import capability_freshness as freshness


class DeviationModules(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name) / "qemu-virt-deviations.json"
        self.child = self.root.parent / "qemu-virt-deviations-recovery.json"
        entry = {key: "value" for key in ("id", "area", "qemu_behavior", "bridgevm_behavior", "impact", "evidence")}
        entry["guest_visible"] = True
        self.parent = {"schema_version": 1, "contract": "qemu-virt-compatible", "deviations": []}
        self.module = {**self.parent, "deviations": [entry]}
        self.parent["deviation_modules"] = [self.child.name]

    def check(self):
        self.root.write_text(json.dumps(self.parent))
        self.child.write_text(json.dumps(self.module))
        CHECK.validate(self.root)

    def test_valid_module(self):
        self.check()

    def test_duplicate_id(self):
        self.parent["deviations"] = self.module["deviations"]
        with self.assertRaisesRegex(ValueError, "duplicate deviation ID"):
            self.check()

    def test_duplicate_or_nonlocal_module(self):
        for names in [[self.child.name, self.child.name], ["../" + self.child.name], [str(self.child)], ["sub\\recovery.json"], ["recovery.json"]]:
            self.parent["deviation_modules"] = names
            with self.assertRaisesRegex(ValueError, "same-directory"):
                self.check()

    def test_metadata_mismatch(self):
        for key, value in [("schema_version", 2), ("contract", "other")]:
            old = self.module[key]
            self.module[key] = value
            with self.assertRaisesRegex(ValueError, "metadata differs"):
                self.check()
            self.module[key] = old

    def test_nested_module(self):
        self.module["deviation_modules"] = []
        with self.assertRaisesRegex(ValueError, "nested"):
            self.check()

    def test_missing_module(self):
        self.parent["deviation_modules"] = ["qemu-virt-deviations-missing.json"]
        with self.assertRaises(FileNotFoundError):
            self.check()

    def test_missing_or_invalid_entry_field(self):
        for key, value in [("impact", ""), ("guest_visible", 1)]:
            old = self.module["deviations"][0][key]
            self.module["deviations"][0][key] = value
            with self.assertRaises(ValueError):
                self.check()
            self.module["deviations"][0][key] = old

    def test_module_only_change_invalidates_capability_freshness(self):
        root = Path(self.tmp.name)
        def git(*args):
            return subprocess.run(["git", *args], cwd=root, capture_output=True,
                                  text=True, check=True).stdout.strip()
        git("init", "-q")
        git("config", "user.name", "Deviation fixture")
        git("config", "user.email", "fixture@example.invalid")
        git("config", "commit.gpgsign", "false")
        path = "docs/machine-contract/qemu-virt-deviations-device-recovery.json"
        target = root / path
        target.parent.mkdir(parents=True)
        target.write_text("{}\n")
        git("add", ".")
        git("commit", "-qm", "fixture base")
        base = git("rev-parse", "HEAD")
        target.write_text('{"changed": true}\n')
        git("add", ".")
        git("commit", "-qm", "fixture module change")
        self.assertEqual(freshness.code_changed_since(base, root), path)


if __name__ == "__main__":
    unittest.main()
