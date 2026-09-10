"""Headless trace evidence contracts, not native shader or guest proof."""
import argparse
import hashlib
import importlib.util
import json
import os
import pathlib
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

ROOT = pathlib.Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts/live-gates"))
import b6_renderer_trace as evidence

spec = importlib.util.spec_from_file_location("trace_runner", ROOT / "scripts/live-gates/run-b6-renderer-trace.py")
runner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runner)


class TraceContracts(unittest.TestCase):
    def test_exact_log_hash_and_markers(self):
        with tempfile.TemporaryDirectory() as directory:
            path = pathlib.Path(directory) / "log"
            raw = b"FRAG\r\nDCL OUT[0], COLOR\n#version 150\n"
            path.write_bytes(raw)
            value = evidence.log_evidence(path)
            self.assertEqual(value["sha256"], hashlib.sha256(raw).hexdigest())
            self.assertEqual((value["bytes"], value["tgsi_headers"], value["glsl_headers"]), (len(raw), 1, 1))

    def test_oversize_and_symlink_refused(self):
        with tempfile.TemporaryDirectory() as directory:
            path = pathlib.Path(directory) / "log"
            path.write_bytes(b"FRAG\n#version 150\n")
            with self.assertRaises(ValueError):
                evidence.log_evidence(path, limit=4)
            link = path.with_name("link")
            link.symlink_to(path)
            with self.assertRaises(ValueError):
                evidence.log_evidence(link)

    def fixture(self, directory, capture):
        out = pathlib.Path(directory)
        for name, raw in (("scale", b"FRAG\n#version 150\n"), ("capture", capture)):
            (out / name).mkdir()
            (out / name / "run.log").write_bytes(raw)
        value = runner.trace_receipt("trace-test", "a" * 40)
        value.update(evidence_paths=[], run_count=3)
        return out, value

    def test_capture_markers_required_even_if_scale_has_them(self):
        for raw in (b"", b"FRAG\n", b"#version 150\n", b"prefix FRAG suffix\n"):
            with tempfile.TemporaryDirectory() as directory, self.subTest(raw=raw):
                out, value = self.fixture(directory, raw)
                with self.assertRaises(ValueError):
                    evidence.finish_trace(runner.core, value, out)
                self.assertEqual(value["run_count"], 0)
                summary = json.loads((out / "renderer-trace-summary.json").read_text())
                self.assertFalse(summary["valid"])
                self.assertFalse(summary["claim_eligible"])

    def test_success_authenticates_policy_without_promoting(self):
        with tempfile.TemporaryDirectory() as directory:
            out, value = self.fixture(directory, b"venus-win32: TGSI received:venus-win32: FRAG\nvenus-win32: GLSL:venus-win32: #version 140\n")
            evidence.finish_trace(runner.core, value, out)
            self.assertEqual(value["raw_sha256"], runner.core.file_hash(out / "renderer-trace-summary.json"))
            self.assertEqual(value["environment_policy_sha256"], runner.core.file_hash(out / "renderer-trace-policy.json"))
            for key in ("pass", "claim_eligible", "criterion_pass", "capability_promotion"):
                self.assertIs(value[key], False)
            self.assertEqual(value["tier"], evidence.TIER)

    def test_fixed_environment_and_restoration_on_error(self):
        def fail(args, receipt_factory, complete):
            self.assertEqual(os.environ["VREND_DEBUG"], evidence.POLICY["VREND_DEBUG"])
            self.assertEqual(receipt_factory("job", "a" * 40)["tier"], evidence.TIER)
            raise ValueError("fixture")
        with mock.patch.dict(os.environ, {"VREND_DEBUG": "untrusted"}), mock.patch.object(runner.core, "run", side_effect=fail):
            with self.assertRaises(ValueError):
                runner.run_trace(None)
            self.assertEqual(os.environ["VREND_DEBUG"], "untrusted")

    def test_completion_error_is_failed_before_receipt_publication(self):
        with tempfile.TemporaryDirectory() as directory:
            out = pathlib.Path(directory)
            disk, variables = out / "disk", out / "variables"
            disk.write_bytes(b"disk"); variables.write_bytes(b"vars")
            args = argparse.Namespace(out=out, job_id="fixture", input_manifest=out / "manifest", sealed_binary=out / "probe")
            records = {key: (out / key, "a" * 64) for key in ("image", "vars", "binary", "virglrenderer", "moltenvk", "viogpu_dir", "presentmon", "config")}
            core = runner.core
            with mock.patch.object(core.subprocess, "check_output", return_value=("a" * 40 + "\n")), \
                 mock.patch.object(core.subprocess, "run"), mock.patch.object(core.platform, "system", return_value="Darwin"), \
                 mock.patch.object(core.platform, "machine", return_value="arm64"), \
                 mock.patch.object(core, "load_inputs", return_value=(records, {"width":1600, "height":900, "logpixels":96})), \
                 mock.patch.object(core, "clone_pair", return_value=(disk, variables)), \
                 mock.patch.object(core, "file_hash", return_value="a" * 64), mock.patch.object(core, "verify_inputs"), \
                 mock.patch.object(core, "validate_capture", return_value={}):
                def refused(value, destination):
                    raise ValueError("missing trace")
                status = core.run(args, receipt_factory=runner.trace_receipt, complete=refused)
            value = json.loads((out / "receipt.json").read_text())
            self.assertEqual(status, 1)
            self.assertFalse(value["valid"])
            self.assertEqual(value["failure_code"], "diagnostic-failed")
            self.assertEqual(value["tier"], evidence.TIER)
            self.assertIn(evidence.CONFOUNDERS[0], value["known_confounders"])

    def test_cli_requires_manifest_for_trace_tier(self):
        result = subprocess.run(["bash", str(ROOT / "scripts/live-gates/bridgevm-live"), "submit", evidence.TIER], capture_output=True, text=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("needs --input-manifest", result.stderr)


if __name__ == "__main__":
    unittest.main()
