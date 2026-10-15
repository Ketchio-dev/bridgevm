#!/usr/bin/env python3
"""Pre-publication failures use synthetic files, including no-writer FIFO."""
import os
import unittest
from t12_sealed_test_support import Fixture

class Refusals(unittest.TestCase):
    def setUp(self): self.f = Fixture()
    def tearDown(self): self.f.close()

    def refused(self):
        result = self.f.submit("refused")
        self.assertNotEqual(result.returncode, 0, result.stdout)
        self.assertFalse(self.f.queue.exists(), result.stderr)
        self.assertFalse(self.f.marker.exists())

    def test_manifest_fields_and_recipe_bindings(self):
        cases = {"schema": ["other"], "binary_source_commit": ["0" * 40],
                 "binary_profile": ["release"], "binary_features": ["venus"],
                 "rust_toolchain": ["ambient"], "image_recipe": ["other"],
                 "image_sha256": ["0" * 64], "initial_vars_recipe": ["other"],
                 "initial_vars_sha256": ["0" * 64], "firmware": [str(self.f.firmware), "0" * 64]}
        for key, value in cases.items():
            with self.subTest(key=key):
                original = self.f.rows[key]; self.f.rows[key] = value
                self.f.write_manifest(); self.refused(); self.f.rows[key] = original

    def test_duplicate_missing_unknown_and_oversized_manifest(self):
        original = self.f.manifest.read_text()
        for content in [original + original.splitlines()[0] + "\n", "\n".join(original.splitlines()[1:]) + "\n",
                        original + "unknown\tvalue\n", original + "x" * 16384]:
            with self.subTest(length=len(content)):
                self.f.manifest.write_text(content); self.refused()

    def test_changed_binary_and_firmware(self):
        for path in [self.f.binary, self.f.firmware]:
            with self.subTest(path=path.name):
                original = path.read_bytes(); path.write_bytes(b"changed")
                self.refused(); path.write_bytes(original)

    def test_nonregular_and_aliased_artifacts_do_not_block(self):
        alias = self.f.assets / "alias"; alias.symlink_to(self.f.firmware)
        fifo = self.f.assets / "fifo"; os.mkfifo(fifo)
        for path in [alias, fifo, self.f.assets]:
            with self.subTest(path=path.name):
                self.f.rows["firmware"][0] = str(path); self.f.write_manifest(); self.refused()

    def test_runner_must_be_executable(self):
        self.f.binary.chmod(0o600); self.refused()

if __name__ == "__main__": unittest.main()
