#!/usr/bin/env python3
"""One sealed T19 manifest row, and nothing else, turns the BitLocker restart check on, and only the
helper launch changes (docs/history/windows-hvf/bitlocker-restart-check-20261023.md)."""
from __future__ import annotations

import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
GATES = ROOT / "scripts/live-gates"
SPEC = importlib.util.spec_from_file_location("t19_switch_manifest", GATES / "windows-import-product-e2e-manifest.py")
MANIFEST = importlib.util.module_from_spec(SPEC); SPEC.loader.exec_module(MANIFEST)
ROW = "bitlocker_restart_check\ttpm-only"
FLAG = "--bitlocker-restart-check"
RELATIONS = {"app_executable": "Contents/MacOS/BridgeVMControl",
             "product_helper": "Contents/Helpers/BridgeVMProductE2E.app/Contents/MacOS/BridgeVMProductE2E",
             "runner": "Contents/Resources/target/release/hvf-runner"}


def sealed_inputs(root: Path) -> list[str]:
    """Real files for every sealed asset and the manifest rows that name them."""
    app = root / "BridgeVM.app"
    for relative in RELATIONS.values():
        (app / relative).parent.mkdir(parents=True, exist_ok=True)
        (app / relative).write_bytes(relative.encode())
    paths = {"app_bundle": app, **{key: app / relative for key, relative in RELATIONS.items()},
             "source_disk": root / "windows.raw", "source_vars": root / "vars.fd", "source_vtpm": root / "vtpm",
             "source_vtpm_package": root / "vtpm-recovery.json", "source_vtpm_code": root / "vtpm-recovery-code.txt"}
    paths["source_disk"].write_bytes(b"disk")
    with paths["source_vars"].open("wb") as variables:
        variables.truncate(64 * 1024 * 1024)
    paths["source_vtpm"].mkdir()
    (paths["source_vtpm"] / "tpm2-00.permall").write_bytes(b"state")
    paths["source_vtpm_package"].write_bytes(b"{}")
    paths["source_vtpm_code"].write_bytes(b"code")
    rows = []
    for key in MANIFEST.ASSETS:
        path = paths[key]
        digest = MANIFEST.tree_hash(path, allow_symlinks=key == "app_bundle") if path.is_dir() else MANIFEST.file_hash(path)
        rows.append(f"{key}\t{path}\t{digest}")
    return rows


class BitLockerSwitchTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.temporary = tempfile.TemporaryDirectory()
        cls.root = Path(cls.temporary.name)
        cls.assets = sealed_inputs(cls.root)

    @classmethod
    def tearDownClass(cls):
        cls.temporary.cleanup()

    def verify(self, *rows: str, mode: str = "pilot") -> dict:
        path = self.root / "manifest.tsv"
        path.write_text("".join(f"{row}\n" for row in (f"campaign_mode\t{mode}", *rows)), encoding="utf-8")
        return MANIFEST.verify(path)

    def test_without_the_row_the_verified_inputs_are_unchanged(self):
        for mode in ("pilot", "release"):
            result = self.verify(*self.assets, mode=mode)
            self.assertTrue(result["verified"], result)
            self.assertEqual(set(result), {"valid", "verified", "campaign_mode", "failure_code", "detail", "assets"})

    def test_one_tpm_only_row_in_a_pilot_manifest_adds_exactly_one_field(self):
        plain = self.verify(*self.assets)
        for rows in ((ROW, *self.assets), (*self.assets, ROW), (self.assets[0], ROW, *self.assets[1:])):
            with self.subTest(position=rows.index(ROW)):
                result = self.verify(*rows)
                self.assertEqual(result, {**plain, "bitlocker_restart_check": "tpm-only"})

    def test_every_other_use_of_the_row_refuses_the_manifest(self):
        cases = {
            "release": ((ROW, *self.assets), "release"), "duplicate": ((ROW, ROW, *self.assets), "pilot"),
            "no value": (("bitlocker_restart_check", *self.assets), "pilot"),
            "empty value": (("bitlocker_restart_check\t", *self.assets), "pilot"),
            "other case": (("bitlocker_restart_check\tTPM-only", *self.assets), "pilot"),
            "trailing space": (("bitlocker_restart_check\ttpm-only ", *self.assets), "pilot"),
            "extra field": ((ROW + "\tyes", *self.assets), "pilot"),
            "pin protector": (("bitlocker_restart_check\ttpm-pin", *self.assets), "pilot"),
            "recovery password": (("bitlocker_restart_check\trecovery-password", *self.assets), "pilot"),
            "near name": (("bitlocker_restart\ttpm-only", *self.assets), "pilot"),
        }
        for name, (rows, mode) in cases.items():
            with self.subTest(name):
                result = self.verify(*rows, mode=mode)
                self.assertEqual((result["valid"], result["verified"], result["failure_code"]), (False, False, "internal-error"))
                self.assertNotIn("bitlocker_restart_check", result)
        self.assertNotIn("bitlocker_restart_check", self.verify(ROW, *self.assets[:-1]))

    def test_the_command_line_preflight_blocks_a_refused_row(self):
        path = self.root / "release.tsv"
        path.write_text("".join(f"{row}\n" for row in ("campaign_mode\trelease", ROW, *self.assets)))
        out = self.root / "verified-release.json"
        completed = subprocess.run([sys.executable, str(GATES / "windows-import-product-e2e-manifest.py"), "--manifest", str(path),
                                    "--out", str(out)], capture_output=True, text=True, check=False)
        self.assertEqual(completed.returncode, 1)
        self.assertIn("A9 import input preflight blocked: internal-error (the BitLocker restart check needs one tpm-only row", completed.stderr)
        self.assertNotIn("bitlocker_restart_check", json.loads(out.read_text()))

    def launch(self, verified: object) -> list[str]:
        """The arguments the lane launcher hands the real launcher, which is replaced by a recorder."""
        directory = Path(tempfile.mkdtemp(dir=self.root))
        for name in ("launch-import-product-e2e-lane.sh", "product_e2e_json_value.py"):
            shutil.copy2(GATES / name, directory / name)
        recorder = directory / "launch-import-product-e2e-helper.sh"
        recorder.write_text('#!/bin/bash\nprintf "%s\\n" "$@"\n'); recorder.chmod(0o755)
        path = directory / "verified-inputs.json"
        if verified is not None:
            path.write_text(verified if isinstance(verified, str) else json.dumps(verified))
        completed = subprocess.run(["/bin/bash", str(directory / "launch-import-product-e2e-lane.sh"), str(path),
                                    "/A/helper", "/P/log", "/W/request.json", "/P/result.json"], capture_output=True, text=True, check=True)
        return completed.stdout.splitlines()

    def test_the_lane_launcher_adds_the_flag_only_for_the_sealed_value(self):
        base = ["/A/helper", "/P/log", "/W/request.json", "/P/result.json"]
        plain = self.verify(*self.assets)
        self.assertEqual(self.launch(plain), base)
        self.assertEqual(self.launch({**plain, "bitlocker_restart_check": "tpm-only"}), base + [FLAG])
        for verified in ({**plain, "bitlocker_restart_check": "yes"}, {**plain, "bitlocker_restart_check": True},
                         {**plain, "bitlocker_restart_check": ["tpm-only"]}, "not json", None):
            with self.subTest(verified=verified):
                self.assertEqual(self.launch(verified), base)

    def test_the_launcher_forwards_only_what_it_is_given(self):
        directory = Path(tempfile.mkdtemp(dir=self.root))
        helper = directory / "BridgeVMProductE2E.app/Contents/MacOS/BridgeVMProductE2E"
        helper.parent.mkdir(parents=True)
        recorder = directory / "bin/open"
        recorder.parent.mkdir(); recorder.write_text('#!/bin/bash\nprintf "%s\\n" "$@"\n'); recorder.chmod(0o755)
        environment = {**os.environ, "PATH": f"{recorder.parent}:/usr/bin:/bin"}
        def launched(*extra: str) -> list[str]:
            return subprocess.run(["/bin/bash", str(GATES / "launch-import-product-e2e-helper.sh"), str(helper), "/P/log",
                                   "/W/request.json", "/P/result.json", *extra], env=environment, capture_output=True,
                                  text=True, check=True).stdout.splitlines()
        base = ["-W", "-n", str(helper.parent.parent.parent), "--stdout", "/P/log", "--stderr", "/P/log", "--args",
                "--windows-import-product-e2e", "--request", "/W/request.json", "--result", "/P/result.json"]
        self.assertEqual(launched(), base)
        self.assertEqual(launched(FLAG), base + [FLAG])

    def test_the_tier_hands_its_verified_inputs_to_the_lane_launcher(self):
        tier = (GATES / "run-windows-import-product-e2e-tier.sh").read_text(encoding="utf-8")
        self.assertEqual(tier.count('"$REPO/scripts/live-gates/launch-import-product-e2e-lane.sh" "$VERIFIED" "$HELPER" '
                                    '"$PRIVATE/lane-$lane-helper.log" "$request" "$result"; helper_status=$?'), 1)
        self.assertNotIn("launch-import-product-e2e-helper.sh", tier)
        self.assertNotIn("bitlocker", tier.lower())


if __name__ == "__main__":
    result = unittest.main(exit=False).result
    raise SystemExit(0 if result.wasSuccessful() else 1)
