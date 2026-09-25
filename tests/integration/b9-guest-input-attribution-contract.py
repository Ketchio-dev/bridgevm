#!/usr/bin/env python3
"""Synthetic source, share and guest-copy attribution contracts for B9."""

from __future__ import annotations

import hashlib
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch


REPO = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO / "scripts/live-gates"))
from b9_real_workload_inputs import KEYS, committed_blob_sha256, stable_file
import b9_real_workload_inputs as inputs
from b9_public_receipt_contract import DIRECT as PUBLIC_DIRECT, STAGED, check_staged_hashes
from b9_share_asset_integrity import DIRECT, check_shared_assets


class GuestAttributionContract(unittest.TestCase):
    def setUp(self):
        temp = tempfile.TemporaryDirectory(dir=str(Path(tempfile.gettempdir()).resolve()))
        self.addCleanup(temp.cleanup)
        self.root = Path(temp.name).resolve(strict=True)
        self.source = self.root / "source"
        self.share = self.root / "share"
        self.source.mkdir()
        self.share.mkdir()
        self.records = {}
        self.staged = {}
        for key, name in DIRECT.items():
            raw = ("sealed-" + key).encode()
            source, shared = self.source / name, self.share / name
            source.write_bytes(raw)
            shared.write_bytes(raw)
            self.records[key] = (source, hashlib.sha256(raw).hexdigest())
            self.staged[name] = stable_file(shared)

    def test_host_rechecks_every_mutable_share_asset(self):
        self.assertEqual(len(KEYS), 15)
        self.assertEqual(len(DIRECT), 5)
        check_shared_assets(self.records, self.staged, self.share)
        for name in DIRECT.values():
            with self.subTest(name=name):
                target = self.share / name
                original = target.read_bytes()
                target.write_bytes(b"X" + original[1:])
                with self.assertRaisesRegex(ValueError, "mutable shared asset"):
                    check_shared_assets(self.records, self.staged, self.share)
                target.write_bytes(original)
        check_shared_assets(self.records, self.staged, self.share)

    def test_host_refuses_missing_alias_and_stale_source(self):
        name = DIRECT["guest_helper_script"]
        target = self.share / name
        original = target.read_bytes()
        target.unlink()
        with self.assertRaises((OSError, ValueError)):
            check_shared_assets(self.records, self.staged, self.share)
        target.symlink_to(self.source / name)
        with self.assertRaisesRegex(ValueError, "symlink"):
            check_shared_assets(self.records, self.staged, self.share)
        target.unlink()
        target.write_bytes(original)
        alias = self.share / "extra-hardlink"
        os.link(target, alias)
        with self.assertRaisesRegex(ValueError, "aliases"):
            check_shared_assets(self.records, self.staged, self.share)
        alias.unlink()
        source = self.records["media"][0]
        source.write_bytes(b"Z" + source.read_bytes()[1:])
        with self.assertRaisesRegex(ValueError, "sealed source"):
            check_shared_assets(self.records, self.staged, self.share)

    def test_oversized_stage_removes_only_its_own_partial_copy(self):
        source, target = self.source / "growing.bin", self.share / "bounded.bin"
        source.write_bytes(b"two bytes")
        with patch.object(inputs, "CHUNK_BYTES", 1):
            with self.assertRaisesRegex(ValueError, "transfer bound"):
                inputs._exclusive_copy(source, target)
            self.assertFalse(target.exists())
            target.write_bytes(b"preexisting")
            with self.assertRaises(FileExistsError):
                inputs._exclusive_copy(source, target)
            self.assertEqual(target.read_bytes(), b"preexisting")

    def test_parser_refuses_bytes_different_from_queue_manifest_seal(self):
        manifest = self.root / "input-manifest.tsv"
        manifest.write_bytes(b"incomplete synthetic manifest\n")
        head = subprocess.check_output(["/usr/bin/git", "-C", str(REPO), "rev-parse", "HEAD"],
                                       text=True).strip()
        with self.assertRaisesRegex(ValueError, "queue-sealed B9 manifest differs"):
            inputs.load_inputs(manifest, head, expected_manifest_sha="0" * 64)

    def test_public_staged_helper_is_bound_to_queue_asset_hash(self):
        self.assertEqual(PUBLIC_DIRECT[DIRECT["guest_helper_script"]], "guest_helper_script")
        self.assertEqual(len(STAGED), 16)
        hashes = {key: hashlib.sha256(key.encode()).hexdigest() for key in KEYS}
        staged = {name: hashes[key] for name, key in PUBLIC_DIRECT.items()}
        staged.update({name: "0" * 64 for name in STAGED - set(staged)})
        receipt = {"result_class": "VLC_PID_PRESENTS_CAPTURED",
                   "asset_hashes": hashes, "staged_file_hashes": staged}
        check_staged_hashes(receipt)
        staged[DIRECT["guest_helper_script"]] = "0" * 64
        with self.assertRaisesRegex(ValueError, "staged share"):
            check_staged_hashes(receipt)

    def test_exact_head_guest_code_and_private_launch_source(self):
        control = REPO / "scripts/win-assets/bv-b9-control.ps1"
        child = REPO / "scripts/win-assets/bv-b9-vlc-playback.ps1"
        helper = REPO / "scripts/win-assets/bv-b9-private-inputs.ps1"
        for path in (control, child, helper):
            raw = path.read_bytes()
            self.assertEqual(raw.count(b"\n"), raw.count(b"\r\n"))
            self.assertLess(len(raw), 8_000_000)
            self.assertEqual(committed_blob_sha256(path), hashlib.sha256(raw).hexdigest())
        child_text = child.read_text()
        helper_hash = hashlib.sha256(helper.read_bytes()).hexdigest()
        self.assertIn("$helperHash = '" + helper_hash + "'", child_text)
        self.assertLess(child_text.index("$self = Get-Item -LiteralPath $PSCommandPath"),
                        child_text.index("Set-StrictMode"))
        self.assertLess(child_text.index("$helperCopy = Get-Item"), child_text.index(". $helperPrivate"))
        self.assertLess(child_text.index("Copy-B9PrivateInput $shareMediaPath"),
                        child_text.index("Invoke-CimMethod -ClassName Win32_Process"))
        self.assertRegex(child_text, re.escape("$mediaPath = Join-Path $work 'bbb_1080p_10s_5MB_av1.webm'"))
        self.assertIn("$presentMonPath = Join-Path $work 'PresentMon-2.5.1-x64.exe'", child_text)
        self.assertIn("Start-Process -FilePath $presentMonPath", child_text)
        self.assertIn("-ExpectedGuestScriptSha256 $ExpectedGuestScriptSha256", control.read_text())
        helper_text = helper.read_text()
        self.assertIn("[IO.FileMode]::CreateNew", helper_text)
        self.assertIn("Assert-Hash $Target $Expected", helper_text)


if __name__ == "__main__":
    unittest.main(verbosity=2)
