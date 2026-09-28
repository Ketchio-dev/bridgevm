#!/usr/bin/env python3
"""Retained negative fixtures for four offline B8 observation corrections."""
from __future__ import annotations

import copy
import hashlib
import io
import os
from pathlib import Path
import runpy
import struct
import sys
import tarfile
import unittest
from unittest import mock

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts/live-gates"))
import b8_clean_install_files as files
import b8_clean_install_inputs as inputs
import b8_clean_install_receipt as receipt

fixture_module = runpy.run_path(str(ROOT / "tests/integration/b8-clean-install-tier-contract.py"))
write_json = fixture_module["write_json"]
digest = fixture_module["digest"]


class AdversarialB8(unittest.TestCase):
    def setUp(self):
        self.fixture = fixture_module["OfflineB8"]("test_sealed_offline_release_and_missing_receipt")
        self.fixture.setUp()
        self.addCleanup(self.fixture.doCleanups)

    def test_unbound_host_and_installed_observations_are_rejected(self):
        f = self.fixture
        private, _ = f.envelopes()
        attacks = (("host_model", "Mac17,9"), ("macos_build", "27.0"),
                   ("signing_class", "ad-hoc"), ("app_absent_before", True),
                   ("app_present_after", True), ("codesign_verified", True),
                   ("worker_cleanup_verified", True),
                   ("installed_app_tree_sha256", f.expected["app_tree_sha256"]),
                   ("installed_executable_sha256", f.expected["app_executable_sha256"]),
                   ("installer_bootstrap_sha256", "b" * 64),
                   ("installer_source_sha256", "b" * 64),
                   ("outcome", "failed"))
        for field, value in attacks:
            with self.subTest(field=field), self.assertRaises(ValueError):
                receipt.public_view({**private, field: value}, f.fields, f.manifest_value,
                                    f.expected, f.job / "diagnostic")
        combined = {**private, "host_model": "Mac17,9", "macos_build": "27.0",
                    "signing_class": "ad-hoc", "codesign_verified": True,
                    "worker_cleanup_verified": True,
                    "installed_app_tree_sha256": f.expected["app_tree_sha256"]}
        with self.assertRaises(ValueError):
            receipt.public_view(combined, f.fields, f.manifest_value,
                                f.expected, f.job / "diagnostic")

    def test_self_consistent_wrong_registry_contract_is_rejected(self):
        f = self.fixture
        contract_path = f.assets / "BridgeVM-release.json"
        sums_path = f.assets / "SHA256SUMS"
        original = copy.deepcopy(f.contract["capability_registry"])
        attacks = ({**original, "path": "../wrong"},
                   {**original, "tested_commit": "c" * 40},
                   {**original, "canonical_json_sha256": "0" * 64},
                   {key: value for key, value in original.items() if key != "reviewed"})
        for forged in attacks:
            with self.subTest(forged=forged):
                contract = {**f.contract, "capability_registry": forged}
                write_json(contract_path, contract)
                sums_path.write_text(
                    f"{digest(contract_path)}  BridgeVM-release.json\n"
                    f"{digest(f.assets / f.tar_name)}  {f.tar_name}\n", encoding="ascii")
                manifest = {**f.manifest_value, "release_contract_sha256": digest(contract_path),
                            "sha256s_sha256": digest(sums_path)}
                with self.assertRaises(ValueError):
                    inputs.verify_release(manifest, f.assets)

    def test_source_registry_reader_ignores_hostile_path_git(self):
        f = self.fixture
        fake = f.root / "fake-bin"
        fake.mkdir()
        marker = f.root / "fake-git-ran"
        script = fake / "git"
        script.write_text(f"#!/bin/sh\n/usr/bin/touch '{marker}'\nexit 66\n", encoding="ascii")
        script.chmod(0o755)
        with mock.patch.dict(os.environ, {"PATH": str(fake) + ":" + os.environ.get("PATH", "")}):
            self.assertEqual(inputs.verify_release(f.manifest_value, f.assets), f.expected)
        self.assertFalse(marker.exists())

    def test_appledouble_header_table_and_payload_bounds(self):
        provenance = b"abc"
        total = 152 + len(provenance)
        header = struct.pack(">II16sH", 0x00051607, 0x00020000, b"Mac OS X        ", 2)
        entries = struct.pack(">III", 9, 50, total - 50) + struct.pack(">III", 2, total, 0)
        payload = (bytes(34) + b"ATTR" + bytes(4) + struct.pack(">III", total, 152, len(provenance))
                   + bytes(14) + struct.pack(">HII", 1, 152, len(provenance)) + bytes(2)
                   + bytes([21]) + b"com.apple.provenance\x00" + provenance)
        valid = header + entries + payload
        inputs._appledouble(valid, provenance)
        corrupt = (valid[:8], valid[:24] + b"\x00\x01" + valid[26:],
                   valid[:30] + struct.pack(">I", 49) + valid[34:],
                   valid[:34] + struct.pack(">I", 31) + valid[38:],
                   valid[:26] + struct.pack(">I", 3) + valid[30:],
                   valid[:131] + b"X" + valid[132:], valid[:-1])
        for raw in corrupt:
            with self.subTest(size=len(raw)), self.assertRaises(ValueError):
                inputs._appledouble(raw, provenance)
        with self.assertRaises(ValueError):
            inputs._appledouble(valid, b"different")

    def test_copy_refuses_growth_and_removes_partial_output(self):
        f = self.fixture
        source = f.root / "growing.tar"
        source.write_bytes(b"before sealed source data")
        destination = f.root / "pinned.tar"
        actual_fdopen = files.os.fdopen

        class GrowingSource:
            def __init__(self, stream):
                self.stream = stream
                self.grew = False
            def __enter__(self):
                self.stream.__enter__()
                return self
            def __exit__(self, *args):
                return self.stream.__exit__(*args)
            def fileno(self):
                return self.stream.fileno()
            def read(self, count):
                if not self.grew:
                    self.grew = True
                    with source.open("ab") as output:
                        output.write(b" adversarial growth")
                return self.stream.read(count)

        with mock.patch.object(files.os, "fdopen", side_effect=lambda *args: GrowingSource(actual_fdopen(*args))):
            with self.assertRaises(ValueError):
                files._copy_tar(source, destination, hashlib.sha256(source.read_bytes()).hexdigest(), 1024)
        self.assertFalse(destination.exists())
        stable = f.root / "stable.tar"
        stable.write_bytes(b"exact sealed tar bytes")
        files._copy_tar(stable, destination, digest(stable), 1024)
        self.assertEqual(destination.read_bytes(), stable.read_bytes())
        destination.write_bytes(b"do not delete preexisting")
        with self.assertRaises(FileExistsError):
            files._copy_tar(stable, destination, digest(stable), 1024)
        self.assertEqual(destination.read_bytes(), b"do not delete preexisting")

    def test_archive_expansion_and_tarball_bounds(self):
        f = self.fixture
        buffer = io.BytesIO()
        with tarfile.open(fileobj=buffer, mode="w:") as archive:
            root = tarfile.TarInfo("BridgeVM.app")
            root.type = tarfile.DIRTYPE
            archive.addfile(root)
            item = tarfile.TarInfo("BridgeVM.app/payload")
            item.size = 2
            archive.addfile(item, io.BytesIO(b"xx"))
        with mock.patch.object(inputs, "MAX_EXPANDED", 1):
            with tarfile.open(fileobj=io.BytesIO(buffer.getvalue()), mode="r:") as archive:
                with self.assertRaises(ValueError):
                    inputs._members(archive)
        with mock.patch.object(inputs, "MAX_TARBALL", 1):
            with self.assertRaises(ValueError):
                inputs.verify_release(f.manifest_value, f.assets)


if __name__ == "__main__":
    unittest.main()
