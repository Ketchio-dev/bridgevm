"""Synthetic startup-log contracts; no guest or glyph criterion proof."""
import argparse
import contextlib
import hashlib
import importlib.util
import json
import os
import pathlib
import sys
import tempfile
import unittest
from unittest import mock

ROOT = pathlib.Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts/live-gates"))


def load(name, filename):
    spec = importlib.util.spec_from_file_location(name, ROOT / "scripts/live-gates" / filename)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


cell = load("b6_cell_guard_runner", "run-b6-cell-observation.py")
trace = load("b6_trace_guard_runner", "run-b6-renderer-trace.py")
redactor = load("b6_guard_redactor", "redact-receipt.py")


class UntracedStartupContracts(unittest.TestCase):
    def exercise(self, scale_log=b"scale ready\n", capture_log=b"capture ready\n", trace_mode=False,
                 missing_stage=None):
        with tempfile.TemporaryDirectory() as directory:
            out = pathlib.Path(directory)
            manifest = out / "manifest.tsv"
            manifest.write_bytes(b"sealed fixture")
            disk, variables = out / "disk.raw", out / "vars.fd"
            disk.write_bytes(b"disk fixture")
            variables.write_bytes(b"vars fixture")
            args = argparse.Namespace(out=out, job_id="b6-startup-contract", input_manifest=manifest,
                                      sealed_binary=out / "binary")
            records = {key: (out / key, "a" * 64) for key in
                       ("image", "vars", "binary", "virglrenderer", "moltenvk", "viogpu_dir",
                        "presentmon", "config", "render_server")}
            config = {"width": 1600, "height": 900, "logpixels": 96}
            core = trace.core if trace_mode else cell
            calls = []

            def fake_run(command, **kwargs):
                if "--out" not in command:
                    return
                stage_path = pathlib.Path(command[command.index("--out") + 1])
                stage_path.mkdir()
                calls.append((stage_path.name, dict(kwargs["env"])))
                if stage_path.name != missing_stage:
                    data = scale_log if stage_path.name == "scale" else capture_log
                    (stage_path / "run.log").write_bytes(data)

            with contextlib.ExitStack() as stack:
                stack.enter_context(mock.patch.dict(os.environ, {"VREND_DEBUG": "foreign-debug"}))
                stack.enter_context(mock.patch.object(core.subprocess, "check_output", return_value="a" * 40))
                stack.enter_context(mock.patch.object(core.subprocess, "run", side_effect=fake_run))
                stack.enter_context(mock.patch.object(core.platform, "system", return_value="Darwin"))
                stack.enter_context(mock.patch.object(core.platform, "machine", return_value="arm64"))
                stack.enter_context(mock.patch.object(core, "load_inputs", return_value=(records, config)))
                stack.enter_context(mock.patch.object(core, "verify_renderer_runtime"))
                stack.enter_context(mock.patch.object(core, "clone_pair", return_value=(disk, variables)))
                stack.enter_context(mock.patch.object(core, "validate_capture", return_value={"run_count": 3}))
                stack.enter_context(mock.patch.object(core, "verify_inputs"))
                status = trace.run_trace(args) if trace_mode else core.run(args)
                if trace_mode:
                    self.assertEqual(os.environ["VREND_DEBUG"], "foreign-debug")
            raw = (out / "receipt.json").read_text()
            receipt = json.loads(raw)
            self.assertNotIn(str(out), raw)
            public = redactor.redact(receipt)
            self.assertNotIn(str(out), json.dumps(public))
            self.assertNotIn("renderer_debug_policy", public)
            self.assertNotIn("renderer_stage_logs", public)
            return status, receipt, calls, raw

    def test_clean_untraced_logs_are_authenticated_without_a_claim(self):
        status, receipt, calls, _ = self.exercise()
        self.assertEqual(status, 0)
        self.assertEqual([name for name, _ in calls], ["scale", "capture"])
        self.assertTrue(all("VREND_DEBUG" not in env for _, env in calls))
        self.assertEqual(set(receipt["renderer_stage_logs"]), {"scale", "capture"})
        self.assertEqual(receipt["renderer_stage_logs"]["scale"]["sha256"], hashlib.sha256(b"scale ready\n").hexdigest())
        self.assertEqual(receipt["renderer_debug_policy"], {"VREND_DEBUG": "unset for scale and capture"})
        self.assertTrue(receipt["valid"])
        self.assertEqual(receipt["run_count"], 3)
        self.assertEqual(receipt["required_run_count"], 27)
        for key in ("pass", "claim_eligible", "criterion_pass", "capability_promotion"):
            self.assertIs(receipt[key], False)

    def test_startup_errors_in_either_stage_refuse_a_valid_receipt(self):
        for stage, scale, capture in (("scale", b"proxy: failed to exec /private/secret\n", b"ok\n"),
                                      ("capture", b"ok\n", b"failed to initialize venus renderer\n")):
            with self.subTest(stage=stage):
                status, receipt, calls, raw = self.exercise(scale, capture)
                self.assertEqual(status, 1)
                self.assertEqual(receipt["failure_code"], "renderer-startup-failed")
                self.assertEqual(receipt["run_count"], 0)
                self.assertFalse(receipt["valid"])
                self.assertFalse(receipt["criterion_pass"])
                self.assertNotIn("/private/secret", raw)
                self.assertEqual([name for name, _ in calls], ["scale"] if stage == "scale" else ["scale", "capture"])

    def test_missing_log_error_does_not_reveal_private_path(self):
        status, receipt, _, raw = self.exercise(missing_stage="scale")
        self.assertEqual(status, 1)
        self.assertEqual(receipt["failure_code"], "renderer-startup-failed")
        self.assertEqual(receipt["diagnostic_error"], "scale renderer log unavailable")
        self.assertNotIn("/private/", raw)

    def test_trace_tier_keeps_its_sealed_debug_environment(self):
        capture = b"venus-win32: TGSI received:venus-win32: FRAG\nvenus-win32: GLSL:venus-win32: #version 140\n"
        status, receipt, calls, _ = self.exercise(capture_log=capture, trace_mode=True)
        self.assertEqual(status, 0)
        self.assertEqual([name for name, _ in calls], ["scale", "capture"])
        self.assertTrue(all(env["VREND_DEBUG"] == trace.POLICY["VREND_DEBUG"] for _, env in calls))
        self.assertNotIn("renderer_debug_policy", receipt)
        self.assertTrue(receipt["valid"])
        self.assertFalse(receipt["criterion_pass"])


if __name__ == "__main__":
    unittest.main()
