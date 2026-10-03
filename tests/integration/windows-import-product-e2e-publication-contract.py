#!/usr/bin/env python3
"""Exercise the actual T19 publication boundary using public metadata only."""
from __future__ import annotations

import hashlib
import importlib.util
import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from windows_import_receipt_fixtures import COMMIT, ROOT, VERIFIER, blocked_receipt, receipt

PUBLISHER = ROOT / "scripts/live-gates/publish-receipt.sh"
REDACTOR = ROOT / "scripts/live-gates/redact-receipt.py"
NEW_HASHES = (
    "source_disk_sha256", "source_vars_sha256", "source_vtpm_tree_sha256",
    "imported_initial_disk_sha256", "imported_initial_vars_sha256",
    "imported_initial_vtpm_tree_sha256", "final_vtpm_tree_sha256",
)
NEW_COUNTS = ("source_authenticated_passes", "ui_imported_passes", "imported_media_authenticated_passes")
SIGNING_CLASSES = ("unverified", "development-ad-hoc", "development-signed", "developer-id-notarized")


def snapshot(path: Path) -> tuple:
    info = path.lstat()
    contents = os.readlink(path) if path.is_symlink() else hashlib.sha256(path.read_bytes()).hexdigest()
    return info.st_dev, info.st_ino, info.st_mode, contents


class Publication(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="bridgevm-t19-publication-")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)

    def write(self, directory: Path, value: dict) -> Path:
        directory.mkdir()
        path = directory / "receipt.json"
        path.write_text(json.dumps(value), encoding="utf-8")
        return path

    def publish(self, directory: Path, commit: str = COMMIT):
        return subprocess.run(["bash", str(PUBLISHER), VERIFIER.TIER, str(directory), str(ROOT), commit],
                              capture_output=True, text=True, timeout=10)

    def assert_no_staging(self, directory: Path):
        self.assertEqual(list(directory.glob(".receipt.public.*.json")), [])

    def assert_published(self, directory: Path, value: dict):
        private = directory / "receipt.json"
        before = snapshot(private)
        result = self.publish(directory)
        self.assertEqual(result.returncode, 0, result.stderr)
        public = json.loads((directory / "receipt.public.json").read_text())
        self.assertEqual(public, value)
        self.assertEqual(set(public), VERIFIER.REQUIRED)
        VERIFIER.validate(public, expected_commit=COMMIT)
        self.assertEqual(snapshot(private), before)
        for field in ("claim_eligible", "criterion_pass", "capability_promotion", "clean_machine"):
            self.assertIs(public[field], False)
        self.assert_no_staging(directory)

    def test_completed_pilot_and_release_publish_all_authenticated_fields(self):
        for mode in ("pilot", "release"):
            with self.subTest(mode=mode):
                value = receipt(mode)
                directory = self.root / mode
                self.write(directory, value)
                self.assert_published(directory, value)

    def test_preflight_blocked_receipt_remains_public_failure(self):
        value = blocked_receipt()
        directory = self.root / "blocked"
        self.write(directory, value)
        self.assert_published(directory, value)

    def test_actual_missing_receipt_writer_can_publish_its_failure(self):
        directory = self.root / "missing"
        directory.mkdir()
        (directory / "input-manifest.tsv").write_text("missing\n")
        (directory / "job.env").write_text("submitted_at=2026-09-16T00:00:00Z\n")
        result = subprocess.run(["bash", str(ROOT / "scripts/live-gates/write-windows-import-product-e2e-missing-receipt.sh"),
                                 str(directory), str(ROOT), "missing-import", COMMIT],
                                capture_output=True, text=True, timeout=10)
        self.assertEqual(result.returncode, 0, result.stderr)
        value = json.loads((directory / "receipt.json").read_text())
        self.assertIs(value["pass"], False)
        self.assertEqual(value["outcome"], "missing-receipt")
        self.assert_published(directory, value)

    def test_recognized_signing_classes_are_accepted_without_promotion(self):
        for signing in SIGNING_CLASSES:
            with self.subTest(signing=signing):
                value = receipt()
                value["artifact_signing_class"] = signing
                self.assertIs(VERIFIER.validate(value)["claim_eligible"], False)

    def test_unknown_or_untyped_signing_class_is_refused(self):
        for ordinal, signing in enumerate(("trusted", True, None, [], {}, 1)):
            with self.subTest(signing=signing):
                value = receipt()
                value["artifact_signing_class"] = signing
                with self.assertRaises(VERIFIER.ReceiptError):
                    VERIFIER.validate(value)
                directory = self.root / f"bad-signing-{ordinal}"
                self.write(directory, value)
                self.assertNotEqual(self.publish(directory).returncode, 0)
                self.assertFalse(os.path.lexists(directory / "receipt.public.json"))
                self.assert_no_staging(directory)

    def test_new_public_fields_refuse_private_values_and_nested_data(self):
        cases = [(field, value) for field in NEW_HASHES for value in ("/fixture/windows.raw", {"nested": "value"})]
        cases += [(field, {"nested": "value"}) for field in NEW_COUNTS]
        for ordinal, (field, value) in enumerate(cases):
            with self.subTest(field=field, value=value):
                directory = self.root / f"private-{ordinal}"
                payload = receipt()
                payload[field] = value
                source = self.write(directory, payload)
                redacted = directory / "direct-public.json"
                result = subprocess.run([sys.executable, str(REDACTOR), "--in", str(source), "--out", str(redacted)],
                                        capture_output=True, text=True, timeout=10)
                self.assertNotEqual(result.returncode, 0)
                self.assertFalse(os.path.lexists(redacted))
                self.assertNotEqual(self.publish(directory).returncode, 0)
                self.assertFalse(os.path.lexists(directory / "receipt.public.json"))
                self.assert_no_staging(directory)

    def test_unknown_private_fields_are_dropped_by_the_allowlist(self):
        directory = self.root / "unknown-private"
        source = self.write(directory, {"source_disk_sha256": "a" * 64, "disk_path": "/fixture/windows.raw"})
        output = directory / "redacted.json"
        result = subprocess.run([sys.executable, str(REDACTOR), "--in", str(source), "--out", str(output)],
                                capture_output=True, text=True, timeout=10)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(json.loads(output.read_text()), {"source_disk_sha256": "a" * 64})

    def test_schema_valid_secret_metadata_refuses_publication_and_clears_staging(self):
        value = receipt()
        value["host_model"] = "password=synthetic"
        VERIFIER.validate(value, expected_commit=COMMIT)
        directory = self.root / "redactor-refusal"
        source = self.write(directory, value)
        before = snapshot(source)
        result = self.publish(directory)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("non-publishable value", result.stderr)
        self.assertFalse(os.path.lexists(directory / "receipt.public.json"))
        self.assertEqual(snapshot(source), before)
        self.assert_no_staging(directory)

    def test_existing_public_files_and_links_are_preserved(self):
        for kind in ("regular", "hardlink", "symlink", "dangling-symlink"):
            with self.subTest(kind=kind):
                directory = self.root / kind
                private = self.write(directory, receipt())
                public, target = directory / "receipt.public.json", directory / "target"
                target.write_text("preserve-target")
                if kind == "regular":
                    public.write_text("preserve-public")
                elif kind == "hardlink":
                    os.link(target, public)
                else:
                    public.symlink_to(target if kind == "symlink" else directory / "absent")
                before = tuple(map(snapshot, (private, public, target)))
                self.assertNotEqual(self.publish(directory).returncode, 0)
                self.assertEqual(tuple(map(snapshot, (private, public, target))), before)
                self.assert_no_staging(directory)

    def test_mismatched_commit_and_malformed_receipt_are_not_published(self):
        for label, commit, value in (("wrong-commit", "c" * 40, receipt()),
                                     ("missing-field", COMMIT, {key: value for key, value in receipt().items() if key != NEW_HASHES[0]})):
            with self.subTest(case=label):
                directory = self.root / label
                self.write(directory, value)
                self.assertNotEqual(self.publish(directory, commit).returncode, 0)
                self.assertFalse(os.path.lexists(directory / "receipt.public.json"))
                self.assert_no_staging(directory)

    def test_allowlist_dependency_bytes_are_sealed_by_both_nvme_reporters(self):
        dependency = "scripts/live-gates/receipt_public_fields.py"
        for name in ("hvf_nvme_performance_report.py", "hvf_nvme_performance_v2_report.py"):
            with self.subTest(reporter=name):
                spec = importlib.util.spec_from_file_location(name.removesuffix(".py"), ROOT / "scripts" / name)
                reporter = importlib.util.module_from_spec(spec)
                spec.loader.exec_module(reporter)
                fixture = self.root / name
                fixture.mkdir()
                for relative in set(reporter.ANALYZER_FILES) | {dependency}:
                    path = fixture / relative
                    path.parent.mkdir(parents=True, exist_ok=True)
                    path.write_text("synthetic analyzer bytes\n")
                for command in (("init", "-q"), ("add", "-A"),
                                ("-c", "user.name=Fixture", "-c", "user.email=fixture@example.invalid",
                                 "-c", "commit.gpgsign=false", "-c", "core.hooksPath=/dev/null", "commit", "-qm", "fixture")):
                    subprocess.run(["git", "-C", str(fixture), *command], capture_output=True, check=True, timeout=10)
                commit = subprocess.check_output(["git", "-C", str(fixture), "rev-parse", "HEAD"], text=True).strip()
                reporter.ROOT = fixture
                seal = reporter._seal_analyzer(commit)
                path = fixture / dependency
                self.assertEqual(seal["source_sha256"].get(dependency), hashlib.sha256(path.read_bytes()).hexdigest())
                path.write_text("changed analyzer bytes\n")
                with self.assertRaisesRegex(ValueError, "uncommitted bytes"):
                    reporter._seal_analyzer(commit)


if __name__ == "__main__":
    unittest.main()
