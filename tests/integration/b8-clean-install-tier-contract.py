#!/usr/bin/env python3
"""Offline B8 contract fixtures. No live queue, installer, or clean OS is used."""
from __future__ import annotations

import hashlib
import io
import json
import os
from pathlib import Path
import plistlib
import subprocess
import sys
import tarfile
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts/live-gates"))
import b8_clean_install_inputs as inputs
import b8_clean_install_receipt as receipt

COMMIT = "a" * 40
TAG = "v1.2.3"


def write_json(path: Path, value: dict) -> None:
    path.write_text(json.dumps(value, sort_keys=True), encoding="utf-8")


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


class OfflineB8(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="b8-contract-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name).resolve()
        self.assets = self.root / "assets"
        self.assets.mkdir()
        self.app = self.root / "source" / "BridgeVM.app"
        binary = self.app / "Contents/MacOS/bridgevm"
        binary.parent.mkdir(parents=True)
        (self.app / "Contents/Info.plist").write_bytes(plistlib.dumps({
            "CFBundleIdentifier": "dev.bridgevm.control", "CFBundleExecutable": "bridgevm"}))
        binary.write_bytes(b"tiny synthetic app; never an installed release\n")
        binary.chmod(0o755)
        self.tar_name = "BridgeVM-" + TAG + ".tar.gz"
        with tarfile.open(self.assets / self.tar_name, "w:gz") as archive:
            archive.add(self.app, arcname="BridgeVM.app")
        self.contract = {"schema_version": 1, "project": "BridgeVM", "version": TAG,
                         "source_commit": COMMIT, "channel": "general-preview",
                         "product_state": "ENGINEERING_PREVIEW",
                         "macos": {"developer_id_signed": False, "notarized": False},
                         "windows_graphics": {"install_mode": "3d-off",
                                             "kernel_driver_included": False,
                                             "test_signing_required": False,
                                             "product_injection_available": False},
                         "capability_registry": {"path": "capabilities/windows-hvf.json"}}
        write_json(self.assets / "BridgeVM-release.json", self.contract)
        write_json(self.assets / "github-release.json", {"tag_name": TAG, "draft": False,
                                                       "prerelease": False, "assets": [
                                                           {"name": "BridgeVM-release.json"},
                                                           {"name": "SHA256SUMS"},
                                                           {"name": self.tar_name}]})
        write_json(self.assets / "github-commit.json", {"sha": COMMIT})
        (self.assets / "SHA256SUMS").write_text(
            f"{digest(self.assets / 'BridgeVM-release.json')}  BridgeVM-release.json\n"
            f"{digest(self.assets / self.tar_name)}  {self.tar_name}\n", encoding="ascii")
        self.attestation = self.root / "clean-host-attestation.json"
        self.attestation.write_text('{"witness":"synthetic; no clean host"}\n', encoding="utf-8")
        self.manifest = self.root / "input-manifest.json"
        self.manifest_value = {
            "schema": "bridgevm.b8-clean-install-inputs.v1", "source_commit": COMMIT,
            "release_tag": TAG, "release_contract_sha256": digest(self.assets / "BridgeVM-release.json"),
            "sha256s_sha256": digest(self.assets / "SHA256SUMS"),
            "tarball_sha256": digest(self.assets / self.tar_name),
            "clean_host_attestation": str(self.attestation),
            "clean_host_attestation_sha256": digest(self.attestation)}
        write_json(self.manifest, self.manifest_value)
        self.job = self.root / "queued" / "b8-synthetic"
        self.job.mkdir(parents=True)
        ledger = self.root / "job-ledger" / self.job.name
        ledger.mkdir(parents=True)
        fields = {"job_id": self.job.name, "tier": receipt.TIER, "commit": COMMIT,
                  "input_manifest_sha256": digest(self.manifest)}
        self.fields = fields
        body = "".join(f"{key}={value}\n" for key, value in fields.items())
        (self.job / "job.env").write_text(body, encoding="ascii")
        (ledger / "entry.env").write_text(body, encoding="ascii")
        (ledger / "entry.env").chmod(0o400)
        self.expected = inputs.verify_release(self.manifest_value, self.assets)

    def envelopes(self):
        private = receipt.missing(self.fields, self.manifest_value, self.expected)
        public = receipt.public_view(private, self.fields, self.manifest_value,
                                     self.expected, self.job / "diagnostic")
        write_json(self.job / "receipt.json", private)
        write_json(self.job / "receipt.public.json", public)
        return private, public

    def test_sealed_offline_release_and_missing_receipt(self):
        manifest = inputs.load_manifest(self.manifest, COMMIT)
        self.assertEqual(inputs.verify_release(manifest, self.assets), self.expected)
        inputs.verify_installed(self.expected, self.app)
        private, public = self.envelopes()
        receipt.verify_public(self.job, self.manifest, self.assets)
        self.assertEqual(set(private), receipt.PRIVATE)
        self.assertEqual(set(public), receipt.PUBLIC)
        self.assertEqual((public["sample_count"], public["guest_boot_count"]), (0, 0))
        self.assertFalse(public["cell_pass"] or public["criterion_pass"])
        schema = json.loads((ROOT / "schemas/b8-clean-install-receipt-v1.json").read_text())
        self.assertEqual(set(schema["required"]), receipt.PUBLIC)
        self.assertEqual(set(schema["properties"]), receipt.PRIVATE)

    def test_manifest_and_release_identity_refuse_drift(self):
        for key, bad in (("source_commit", "b" * 40), ("release_tag", "../private"),
                         ("extra", "no"), ("tarball_sha256", "b" * 64),
                         ("clean_host_attestation_sha256", "c" * 64)):
            with self.subTest(key=key):
                value = {**self.manifest_value, key: bad}
                write_json(self.manifest, value)
                if key == "tarball_sha256":
                    with self.assertRaises(ValueError):
                        inputs.verify_release(inputs.load_manifest(self.manifest, COMMIT), self.assets)
                else:
                    with self.assertRaises(ValueError):
                        inputs.load_manifest(self.manifest, COMMIT)
        write_json(self.manifest, self.manifest_value)
        for asset, key, bad in (("github-release.json", "draft", True),
                                ("github-release.json", "prerelease", True),
                                ("github-commit.json", "sha", "b" * 40),
                                ("BridgeVM-release.json", "channel", "legacy"),
                                ("BridgeVM-release.json", "source_commit", "b" * 40),
                                ("BridgeVM-release.json", "product_state", "RELEASE")):
            with self.subTest(asset=asset, key=key):
                path = self.assets / asset
                original = path.read_bytes()
                value = json.loads(original)
                value[key] = bad
                write_json(path, value)
                with self.assertRaises(ValueError):
                    inputs.verify_release(self.manifest_value, self.assets)
                path.write_bytes(original)
        sums = self.assets / "SHA256SUMS"
        sums.write_text(sums.read_text() + sums.read_text().splitlines()[0] + "\n")
        with self.assertRaises(ValueError):
            inputs.verify_release(self.manifest_value, self.assets)
        self.attestation.write_bytes(b"changed synthetic witness")
        with self.assertRaises(ValueError):
            inputs.load_manifest(self.manifest, COMMIT)

    def test_archive_and_installed_bundle_fail_closed(self):
        def archive_with(entries):
            buffer = io.BytesIO()
            with tarfile.open(fileobj=buffer, mode="w:") as archive:
                root = tarfile.TarInfo("BridgeVM.app")
                root.type = tarfile.DIRTYPE
                archive.addfile(root)
                for name, kind, target in entries:
                    item = tarfile.TarInfo(name)
                    if kind == "link":
                        item.type = tarfile.SYMTYPE
                        item.linkname = target
                    elif kind == "hardlink":
                        item.type = tarfile.LNKTYPE
                        item.linkname = target
                    else:
                        item.size = len(target)
                    archive.addfile(item, io.BytesIO(target) if kind == "file" else None)
            return buffer.getvalue()
        attacks = [
            [("BridgeVM.app/../escape", "file", b"x")],
            [("BridgeVM.app/link", "link", "../../escape")],
            [("BridgeVM.app/link", "link", "Contents"),
             ("BridgeVM.app/link/overwrite", "file", b"x")],
            [("BridgeVM.app/a", "file", b"x"), ("BridgeVM.app/a", "file", b"y")],
            [("BridgeVM.app/A", "file", b"x"), ("BridgeVM.app/a", "file", b"y")],
            [("BridgeVM.app/link", "hardlink", "BridgeVM.app/Contents")],
        ]
        for entries in attacks:
            with self.subTest(entries=entries):
                with tarfile.open(fileobj=io.BytesIO(archive_with(entries)), mode="r:") as archive:
                    with self.assertRaises(ValueError):
                        inputs._members(archive)
        binary = self.app / "Contents/MacOS/bridgevm"
        binary.write_bytes(b"changed")
        with self.assertRaises(ValueError):
            inputs.verify_installed(self.expected, self.app)
        binary.unlink()
        with self.assertRaises(OSError):
            inputs.verify_installed(self.expected, self.app)

    def test_platform_tar_round_trip_uses_same_app_bytes(self):
        tarball = self.assets / self.tar_name
        subprocess.run(["/usr/bin/tar", "-czf", str(tarball), "-C", str(self.app.parent),
                        "BridgeVM.app"], check=True)
        self.manifest_value["tarball_sha256"] = digest(tarball)
        (self.assets / "SHA256SUMS").write_text(
            f"{digest(self.assets / 'BridgeVM-release.json')}  BridgeVM-release.json\n"
            f"{digest(tarball)}  {self.tar_name}\n", encoding="ascii")
        self.manifest_value["sha256s_sha256"] = digest(self.assets / "SHA256SUMS")
        write_json(self.manifest, self.manifest_value)
        expected = inputs.verify_release(inputs.load_manifest(self.manifest, COMMIT), self.assets)
        installed = self.root / "platform-extracted"
        installed.mkdir()
        subprocess.run(["/usr/bin/tar", "-xzf", str(tarball), "-C", str(installed)], check=True)
        inputs.verify_installed(expected, installed / "BridgeVM.app")

    def test_forged_appledouble_and_pax_refused(self):
        def forged(name, data, pax=None):
            buffer = io.BytesIO()
            with tarfile.open(fileobj=buffer, mode="w:") as archive:
                metadata = tarfile.TarInfo(name)
                metadata.size = len(data)
                metadata.pax_headers = pax or {}
                archive.addfile(metadata, io.BytesIO(data))
                root = tarfile.TarInfo("BridgeVM.app")
                root.type = tarfile.DIRTYPE
                archive.addfile(root)
            with tarfile.open(fileobj=io.BytesIO(buffer.getvalue()), mode="r:") as archive:
                with self.assertRaises(ValueError):
                    inputs._members(archive)
        forged("._BridgeVM.app", b"not AppleDouble")
        forged("BridgeVM.app/._absent", b"\x00\x05\x16\x07\x00\x02\x00\x00")
        forged("BridgeVM.app/../escape", b"x")
        forged("BridgeVM.app/x", b"x", {"path": "BridgeVM.app/x"})
        forged("BridgeVM.app/x", b"x", {"LIBARCHIVE.xattr.com.apple.provenance": "AQI"})
        forged("BridgeVM.app/x", b"x", {"LIBARCHIVE.xattr.com.apple.provenance": "AQI",
                                        "SCHILY.xattr.com.apple.provenance": "wrong"})

    def test_archive_expansion_and_tarball_bounds(self):
        buffer = io.BytesIO()
        with tarfile.open(fileobj=buffer, mode="w:") as archive:
            root = tarfile.TarInfo("BridgeVM.app")
            root.type = tarfile.DIRTYPE
            archive.addfile(root)
            item = tarfile.TarInfo("BridgeVM.app/payload")
            item.size = 2
            archive.addfile(item, io.BytesIO(b"xx"))
        original_expanded = inputs.MAX_EXPANDED
        inputs.MAX_EXPANDED = 1
        try:
            with tarfile.open(fileobj=io.BytesIO(buffer.getvalue()), mode="r:") as archive:
                with self.assertRaises(ValueError):
                    inputs._members(archive)
        finally:
            inputs.MAX_EXPANDED = original_expanded
        original_tarball = inputs.MAX_TARBALL
        inputs.MAX_TARBALL = 1
        try:
            with self.assertRaises(ValueError):
                inputs.verify_release(self.manifest_value, self.assets)
        finally:
            inputs.MAX_TARBALL = original_tarball

    def test_public_private_ledger_and_type_refusal(self):
        private, public = self.envelopes()
        for field, bad in (("sample_count", True), ("guest_boot_count", 1),
                           ("criterion_pass", True), ("capability_promotion", True),
                           ("cell_pass", True), ("clean_machine", True),
                           ("host_model", "/private/host"), ("outcome", "completed"),
                           ("installer_source_sha256", "x" * 64),
                           ("finished_at", "2026-13-25T00:00:00Z"),
                           ("extra", "forged")):
            with self.subTest(field=field):
                write_json(self.job / "receipt.json", {**private, field: bad})
                with self.assertRaises(ValueError):
                    receipt.verify_public(self.job, self.manifest, self.assets)
        write_json(self.job / "receipt.json", private)
        for field, bad in (("input_manifest_sha256", "b" * 64),
                           ("host_model", "/private/path"),
                           ("sample_count", 1), ("extra", "secret")):
            with self.subTest(field=field):
                write_json(self.job / "receipt.public.json", {**public, field: bad})
                with self.assertRaises(ValueError):
                    receipt.verify_public(self.job, self.manifest, self.assets)
        write_json(self.job / "receipt.public.json", public)
        ledger = self.root / "job-ledger" / self.job.name / "entry.env"
        ledger.chmod(0o600)
        with self.assertRaises(ValueError):
            receipt.verify_public(self.job, self.manifest, self.assets)
        ledger.chmod(0o400)
        (self.job / "receipt.json").unlink()
        with self.assertRaises(OSError):
            receipt.verify_public(self.job, self.manifest, self.assets)

    def test_private_artifact_seal_and_job_symlink_refusal(self):
        private, _ = self.envelopes()
        diagnostic = self.job / "diagnostic"
        diagnostic.mkdir()
        log = diagnostic / "installer.log"
        log.write_bytes(b"synthetic failure only\n")
        private["private_artifacts"] = {"installer.log": {"bytes": log.stat().st_size,
                                                          "sha256": digest(log)}}
        write_json(self.job / "receipt.json", private)
        receipt.verify_public(self.job, self.manifest, self.assets)
        log.write_bytes(b"altered failure only\n")
        with self.assertRaises(ValueError):
            receipt.verify_public(self.job, self.manifest, self.assets)
        log.write_bytes(b"synthetic failure only\n")
        shortcut = self.root / "queued" / "b8-shortcut"
        shortcut.symlink_to(self.job, target_is_directory=True)
        with self.assertRaises(ValueError):
            receipt.job_fields(shortcut)

    def test_duplicate_json_and_symlink_inputs_refused(self):
        self.manifest.write_text('{"schema":"one","schema":"two"}', encoding="utf-8")
        with self.assertRaises(ValueError):
            inputs.load_manifest(self.manifest, COMMIT)
        write_json(self.manifest, self.manifest_value)
        link = self.root / "link.json"
        link.symlink_to(self.manifest)
        with self.assertRaises(ValueError):
            inputs.load_manifest(link, COMMIT)
        saved = self.attestation.read_bytes()
        self.attestation.unlink()
        self.attestation.symlink_to(self.manifest)
        with self.assertRaises(ValueError):
            inputs.load_manifest(self.manifest, COMMIT)
        self.attestation.unlink()
        self.attestation.write_bytes(saved)
        self.envelopes()
        private_path = self.job / "receipt.json"
        value = private_path.read_text()
        private_path.write_text('{"schema":"forged",' + value[1:], encoding="utf-8")
        with self.assertRaises(ValueError):
            receipt.verify_public(self.job, self.manifest, self.assets)

    def test_installer_env_excludes_hostile_path_and_bash_env(self):
        fake = self.root / "fake"
        fake.mkdir()
        marker = self.root / "hostile-ran"
        fake_marker = self.root / "fake-helper-ran"
        for name in ("shasum", "codesign"):
            script = fake / name
            script.write_text(f"#!/bin/sh\ntouch {fake_marker}\nexit 17\n", encoding="ascii")
            script.chmod(0o755)
        bash_env = self.root / "bash-env"
        bash_env.write_text(f"touch {marker}\n", encoding="ascii")
        unsafe = {**os.environ, "PATH": str(fake) + ":" + inputs.SYSTEM_PATH,
                  "BASH_ENV": str(bash_env), "BRIDGEVM_INSTALL_ASSET_DIR": "/private/fake"}
        subprocess.run(["/bin/bash", "-c", "shasum /dev/null"], env=unsafe,
                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=False)
        self.assertTrue(marker.exists())
        self.assertTrue(fake_marker.exists())
        marker.unlink()
        fake_marker.unlink()
        home = self.root / "home"
        private = self.root / "private"
        home.mkdir(mode=0o700)
        private.mkdir(mode=0o700)
        clean = inputs.safe_installer_env(home, private)
        self.assertEqual(set(clean), {"PATH", "HOME", "TMPDIR", "LANG"})
        self.assertEqual(clean["PATH"], inputs.SYSTEM_PATH)
        run = subprocess.run(["/bin/bash", "-c", "command -v shasum; command -v codesign"],
                             env=clean, capture_output=True, text=True, check=True)
        self.assertNotIn(str(fake), run.stdout)
        self.assertFalse(marker.exists())
        self.assertFalse(fake_marker.exists())
        private.chmod(0o777)
        with self.assertRaises(ValueError):
            inputs.safe_installer_env(home, private)


if __name__ == "__main__":
    unittest.main()
