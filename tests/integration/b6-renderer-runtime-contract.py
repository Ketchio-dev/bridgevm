#!/usr/bin/env python3
"""Sealed server packaging checks do not emulate a Vulkan workload."""
import importlib.util
import os
import pathlib
import sys
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts/live-gates"))
from b6_renderer_runtime import verify_renderer_runtime
spec = importlib.util.spec_from_file_location("cell_fixture", pathlib.Path(__file__).with_name("b6-cell-observation-contract.py"))
fixture = importlib.util.module_from_spec(spec)
spec.loader.exec_module(fixture)


class RuntimeContracts(unittest.TestCase):
    def setUp(self):
        self.cell = fixture.CellContracts()
        self.cell.setUp()
        self.addCleanup(self.cell.doCleanups)
        self.server = self.cell.paths["render_server"]
        self.server.chmod(0o500)
        self.cell.paths["virglrenderer"].write_bytes(b"prefix\0" + os.fsencode(self.server) + b"\0suffix")
        self.cell.seal()

    def runtime(self, environment=None):
        verify_renderer_runtime(self.cell.records, {} if environment is None else environment)

    def test_complete_runtime_is_accepted(self):
        self.cell.load()
        self.runtime()

    def test_legacy_manifest_without_server_is_refused(self):
        self.cell.manifest.write_text("\n".join(line for line in self.cell.manifest.read_text().splitlines()
                                              if not line.startswith("render_server\t")) + "\n")
        with self.assertRaisesRegex(ValueError, "incomplete"):
            self.cell.load()

    def test_changed_server_hash_is_refused(self):
        self.server.chmod(0o700)
        self.server.write_bytes(b"replacement")
        self.server.chmod(0o500)
        with self.assertRaisesRegex(ValueError, "hash mismatch: render_server"):
            self.cell.load()

    def test_missing_server_is_refused(self):
        self.server.unlink()
        with self.assertRaises(OSError):
            self.cell.load()

    def test_writable_server_is_refused(self):
        self.server.chmod(0o700)
        with self.assertRaisesRegex(ValueError, "immutable"):
            self.runtime()

    def test_nonexecutable_server_is_refused(self):
        self.server.chmod(0o400)
        with self.assertRaisesRegex(ValueError, "executable"):
            self.runtime()

    def test_stale_compiled_path_is_refused(self):
        self.cell.paths["virglrenderer"].write_bytes(b"/unused-prefix/libexec/virgl_render_server\0")
        with self.assertRaisesRegex(ValueError, "sealed server path"):
            self.runtime()

    def test_path_suffix_is_not_an_exact_match(self):
        self.cell.paths["virglrenderer"].write_bytes(b"extra" + os.fsencode(self.server) + b"\0")
        with self.assertRaisesRegex(ValueError, "sealed server path"):
            self.runtime()

    def test_even_empty_override_is_refused(self):
        for value in ("", str(self.server), "/unsealed/server"):
            with self.assertRaisesRegex(ValueError, "override refused"):
                self.runtime({"RENDER_SERVER_EXEC_PATH": value})


if __name__ == "__main__":
    unittest.main()
