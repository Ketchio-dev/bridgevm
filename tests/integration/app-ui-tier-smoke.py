#!/usr/bin/env python3
"""Deterministic d6 contracts: owned queue bytes and reports; never launch UI."""
import copy
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import struct
import subprocess
import tempfile
import unittest
from unittest import mock
import zlib

ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location("app_ui", ROOT / "scripts/live-gates/app_ui_diagnostic.py")
ui = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(ui)
COMMIT = subprocess.check_output(["git", "-C", str(ROOT), "rev-parse", "HEAD"], text=True).strip()
CLI = ROOT / "scripts/live-gates/bridgevm-live"


def png(compressed=None, header=None):
    def chunk(kind, body):
        return struct.pack(">I", len(body)) + kind + body + struct.pack(">I", zlib.crc32(kind + body))
    header = header if header is not None else struct.pack(">IIBBBBB", 1, 1, 8, 6, 0, 0, 0)
    compressed = compressed if compressed is not None else zlib.compress(b"\0\x10\x20\x30\xff")
    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", header)
            + chunk(b"IDAT", compressed) + chunk(b"IEND", b""))


class AppUITierContracts(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="bridgevm app ui contract ")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.environment = dict(os.environ, BRIDGEVM_LIVE_ROOT=str(self.root / "queue"))
        self.binary = self.root / "owned test bundle executable"
        # Header fixture is data only, never a runnable binary or launched job.
        self.binary.write_bytes(struct.pack("<IIIIIIII", 0xFEEDFACF, 0x0100000C, 0, 8, 0, 0, 0, 0))
        self.binary_hash = ui.digest(self.binary)
        self.manifest = self.root / "inputs.tsv"
        self.manifest.write_text(f"format\t{ui.FORMAT}\ncommit\t{COMMIT}\nbinary\t{self.binary}\t{self.binary_hash}\n")
        self.manifest_text = self.manifest.read_text()

    def submit(self, *extra):
        return subprocess.run([str(CLI), "submit", ui.TIER, "--sha", COMMIT,
                               "--input-manifest", str(self.manifest), *extra],
                              env=self.environment, text=True, capture_output=True, timeout=10)

    def report(self):
        captures = []
        for name in ui.SCREENSHOTS:
            path = self.root / (name + ".png")
            path.write_bytes(png())
            captures.append({"name": name, "file": path.name, "sha256": ui.digest(path), "width": 1, "height": 1})
        return {"schema_version": 1, "kind": "native-app-ui-diagnostic", "fixture_data": True,
                "guest_behavior_proof": False, "failure": None, "screenshots": captures,
                "actions": dict.fromkeys(ui.ACTIONS, True), "tripwires": dict.fromkeys(ui.TRIPWIRES, 0)}

    def write_report(self, value):
        (self.root / "ui-observations.json").write_text(json.dumps(value))

    def test_actual_cli_seals_exact_owned_bytes_without_launch(self):
        result = self.submit("--job-id", "AppUI.owned-seal")
        self.assertEqual(result.returncode, 0, result.stderr)
        job = self.root / "queue/queued/AppUI.owned-seal"
        self.assertEqual((job / "input-manifest.tsv").read_bytes(), self.manifest.read_bytes())
        self.assertEqual((job / "hvf_gic_boot_probe").read_bytes(), self.binary.read_bytes())
        self.assertEqual(ui.load_job(job, COMMIT, job / "input-manifest.tsv", self.binary_hash), "AppUI.owned-seal")
        self.binary.write_bytes(b"changed original, sealed copy stays unchanged")
        ui.verify_binary(job / "hvf_gic_boot_probe", self.binary_hash)
        self.assertNotEqual(self.submit("--job-id", "AppUI.owned-seal").returncode, 0)
        self.assertFalse((job / "run.log").exists())
        self.assertFalse((job / "app-ui-private").exists())

    def test_actual_cli_rejects_manifest_and_binary_before_queueing(self):
        malformed = [
            self.manifest_text.replace(ui.FORMAT, "other"),
            self.manifest_text.replace(COMMIT, "0" * 40),
            self.manifest_text + "format\t" + ui.FORMAT + "\n",
            self.manifest_text + "command\t/bin/true\n",
            self.manifest_text.replace(str(self.binary), "relative"),
            self.manifest_text.replace(self.binary_hash, "f" * 64),
            self.manifest_text.replace("binary\t", "binary\textra\t"),
            self.manifest_text.replace("format\t" + ui.FORMAT + "\n", ""),
        ]
        for value in malformed:
            with self.subTest(manifest=value):
                self.manifest.write_text(value)
                self.assertNotEqual(self.submit().returncode, 0)
                self.assertFalse((self.root / "queue").exists())
        self.manifest.write_text(self.manifest_text)
        original = self.binary.read_bytes()
        self.binary.write_bytes(b"not Mach-O")
        self.manifest.write_text(self.manifest_text.replace(self.binary_hash, ui.digest(self.binary)))
        self.assertNotEqual(self.submit().returncode, 0)
        self.binary.write_bytes(original)
        self.manifest.write_text(self.manifest_text)
        linked = self.root / "alias"
        linked.symlink_to(self.binary)
        self.manifest.write_text(self.manifest_text.replace(str(self.binary), str(linked)))
        self.assertNotEqual(self.submit().returncode, 0)

    def test_dispatch_refuses_changed_seal_and_emits_nonpromoting_receipt(self):
        submitted = self.submit("--job-id", "AppUI.changed-seal")
        self.assertEqual(submitted.returncode, 0, submitted.stderr)
        job = self.root / "queue/queued/AppUI.changed-seal"
        (job / "hvf_gic_boot_probe").chmod(0o700)
        (job / "hvf_gic_boot_probe").write_bytes(b"changed sealed input")
        result = subprocess.run([str(ROOT / "scripts/live-gates/run-tier.sh"), ui.TIER,
                                 "--out", str(job), "--job-id", "AppUI.changed-seal",
                                 "--input-manifest", str(job / "input-manifest.tsv"),
                                 "--sealed-binary", str(job / "hvf_gic_boot_probe")],
                                text=True, capture_output=True, timeout=10)
        self.assertNotEqual(result.returncode, 0)
        receipt = json.loads((job / "receipt.json").read_text())
        for field in ("pass", "claim_eligible", "criterion_pass", "capability_promotion"):
            self.assertIs(receipt[field], False)
        self.assertEqual(receipt["boots_attempted"], 0)
        self.assertEqual(receipt["run_count"], 0)
        self.assertFalse((job / "app-ui-private").exists())
        subprocess.run([str(ROOT / "scripts/live-gates/publish-receipt.sh"), ui.TIER, str(job),
                        str(ROOT), COMMIT], check=True, capture_output=True, timeout=10)
        public = (job / "receipt.public.json").read_text()
        self.assertNotIn(str(self.root), public)
        self.assertEqual(json.loads(public)["binary_hash"], self.binary_hash)

    def test_parser_checks_job_commit_and_both_seals(self):
        result = self.submit("--job-id", "AppUI.job-identity")
        self.assertEqual(result.returncode, 0, result.stderr)
        job = self.root / "queue/queued/AppUI.job-identity"
        original = (job / "job.env").read_text()
        for old, new in ((COMMIT, "0" * 40), (self.binary_hash, "0" * 64),
                         (ui.digest(self.manifest), "f" * 64), (ui.TIER, "t17-windows-hvf-product-e2e")):
            (job / "job.env").write_text(original.replace(old, new))
            with self.assertRaises(ValueError):
                ui.load_job(job, COMMIT, job / "input-manifest.tsv", self.binary_hash)
        (job / "job.env").write_text(original + "commit=" + COMMIT + "\n")
        with self.assertRaises(ValueError):
            ui.load_job(job, COMMIT, job / "input-manifest.tsv", self.binary_hash)

    def test_completed_report_authenticates_all_pngs_and_actions(self):
        self.write_report(self.report())
        self.assertEqual(ui.verify_observations(self.root), ui.digest(self.root / "ui-observations.json"))

    def test_report_refuses_missing_actions_work_attempts_and_schema_drift(self):
        baseline = self.report()
        mutations = [lambda v: v.update(schema_version=True), lambda v: v.update(failure="UI unavailable"),
                     lambda v: v.update(guest_behavior_proof=True), lambda v: v.update(extra=True),
                     lambda v: v["screenshots"].pop(),
                     lambda v: v["screenshots"].__setitem__(1, v["screenshots"][0])]
        mutations += [lambda v, key=key: v["actions"].__setitem__(key, False) for key in ui.ACTIONS]
        mutations += [lambda v, key=key: v["tripwires"].__setitem__(key, 1) for key in ui.TRIPWIRES]
        mutations += [lambda v, key=key: v["tripwires"].__setitem__(key, False) for key in ui.TRIPWIRES]
        for mutate in mutations:
            value = copy.deepcopy(baseline)
            mutate(value)
            self.write_report(value)
            with self.assertRaises(ValueError):
                ui.verify_observations(self.root)
        self.write_report(baseline)
        report = self.root / "ui-observations.json"
        report.write_text(report.read_text().replace('"schema_version": 1', '"schema_version": 1, "schema_version": 1'))
        with self.assertRaises(ValueError):
            ui.verify_observations(self.root)

    def test_report_refuses_forged_missing_or_aliased_captures(self):
        baseline = self.report()
        for field, value in (("file", "../other.png"), ("sha256", "0" * 64), ("width", 2), ("height", True)):
            changed = copy.deepcopy(baseline)
            changed["screenshots"][0][field] = value
            self.write_report(changed)
            with self.assertRaises(ValueError):
                ui.verify_observations(self.root)
        self.write_report(baseline)
        image = self.root / baseline["screenshots"][0]["file"]
        image.unlink()
        with self.assertRaises(OSError):
            ui.verify_observations(self.root)
        image.symlink_to(self.root / baseline["screenshots"][1]["file"])
        with self.assertRaises(ValueError):
            ui.verify_observations(self.root)
        image.unlink()
        for content in (b"not a PNG", png()[:-1], png() + b"trailing", png()[:20] + b"changed" + png()[27:]):
            image.write_bytes(content)
            baseline["screenshots"][0]["sha256"] = ui.digest(image)
            self.write_report(baseline)
            with self.assertRaises(ValueError):
                ui.verify_observations(self.root)

    def test_exit_success_without_observations_cannot_pass(self):
        # Exercise the real runner composition with only its child boundary replaced.
        result = self.submit("--job-id", "AppUI.no-report")
        self.assertEqual(result.returncode, 0, result.stderr)
        job = self.root / "queue/queued/AppUI.no-report"
        with mock.patch.object(ui.sys, "platform", "darwin"), mock.patch.object(ui, "run_child", return_value=0) as child:
            code = ui.run(job, ROOT, COMMIT, job / "input-manifest.tsv", job / "hvf_gic_boot_probe")
        self.assertEqual(code, 1)
        self.assertEqual(child.call_count, 1)
        receipt = json.loads((job / "receipt.json").read_text())
        self.assertEqual(receipt["failure_code"], "observation-refused")
        self.assertIs(receipt["pass"], False)

    def test_report_refuses_crc_valid_undecodable_png_images(self):
        baseline = self.report()
        image = self.root / baseline["screenshots"][0]["file"]
        stream = zlib.compress(b"\0\x10\x20\x30\xff")
        variants = [
            png(b"not a zlib image stream"), png(stream[:-1]), png(stream + b"extra stream bytes"),
            png(zlib.compress(b"\0")), png(zlib.compress(b"\0\x10\x20\x30\xff\0")),
            png(zlib.compress(b"\5\x10\x20\x30\xff")),
            png(header=struct.pack(">IIBBBBB", 1, 1, 3, 6, 0, 0, 0)),
            png(header=struct.pack(">IIBBBBB", 1, 1, 8, 6, 1, 0, 0)),
            png(header=struct.pack(">IIBBBBB", 1, 1, 8, 6, 0, 1, 0)),
            png(header=struct.pack(">IIBBBBB", 1, 1, 8, 6, 0, 0, 2)),
            png(header=struct.pack(">IIBBBBB", 8192, 8192, 16, 6, 0, 0, 0)),
        ]
        for data in variants:
            image.write_bytes(data)
            baseline["screenshots"][0]["sha256"] = ui.digest(image)
            self.write_report(baseline)
            with self.assertRaises(ValueError):
                ui.verify_observations(self.root)
        # A legal tiny Adam7 image has one nonempty pass, with one RGBA row.
        image.write_bytes(png(header=struct.pack(">IIBBBBB", 1, 1, 8, 6, 0, 0, 1)))
        baseline["screenshots"][0]["sha256"] = ui.digest(image)
        self.write_report(baseline)
        self.assertEqual(ui.verify_observations(self.root), ui.digest(self.root / "ui-observations.json"))

    def test_cancellation_before_launch_preserves_zero_runs(self):
        result = self.submit("--job-id", "AppUI.canceled")
        self.assertEqual(result.returncode, 0, result.stderr)
        job = self.root / "queue/queued/AppUI.canceled"
        (job / "cancel.requested").touch()
        with mock.patch.object(ui.sys, "platform", "darwin"), mock.patch.object(ui, "run_child") as child:
            self.assertEqual(ui.run(job, ROOT, COMMIT, job / "input-manifest.tsv", job / "hvf_gic_boot_probe"), 1)
        child.assert_not_called()
        receipt = json.loads((job / "receipt.json").read_text())
        self.assertEqual(receipt["run_count"], 0)
        self.assertEqual(receipt["failure_code"], "canceled-or-deadline")

    def test_child_deadline_has_bounded_termination_without_launch(self):
        child = mock.Mock()
        child.poll.return_value = None
        child.wait.side_effect = [subprocess.TimeoutExpired("fixture", 3), -9]
        with mock.patch.object(ui.subprocess, "Popen", return_value=child) as launch, \
                mock.patch.object(ui.time, "monotonic", side_effect=[0, 91]):
            with self.assertRaises(TimeoutError):
                ui.run_child(self.root / "unlaunched.xctest", self.root, lambda: False)
        command = launch.call_args.args[0]
        self.assertEqual(command[:4], ["/usr/bin/xcrun", "xctest", "-XCTest", ui.TEST_CLASS])
        self.assertNotIn("start_new_session", launch.call_args.kwargs)
        child.terminate.assert_called_once_with()
        child.kill.assert_called_once_with()
        self.assertEqual(child.wait.call_args_list, [mock.call(timeout=3), mock.call(timeout=2)])


if __name__ == "__main__":
    unittest.main(verbosity=2)
