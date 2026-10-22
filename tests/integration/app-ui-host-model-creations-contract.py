#!/usr/bin/env python3
"""Exact inert control-model accounting in the native UI observation verifier; no app or job runs.

The seeded overview rows each request one control model; the diagnostic admits exactly one
per seeded fixture VM over an inert backend whose every call is a fatal tripwire. A completed
report must say: none before seeding, exactly the seeded count after it, no backend call.
"""
import copy
import importlib.util
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest import mock

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts/live-gates"))
import app_ui_diagnostic as original
import app_ui_host_diagnostic as host
import app_ui_host_v2_bundle as v2_bundle
import app_ui_host_v2_diagnostic as v2
from app_ui_host_bundle_fixtures import png
from app_ui_host_v2_fixtures import Fixture

SPEC = importlib.util.spec_from_file_location("app_ui_host_tier_smoke", ROOT / "tests/integration/app-ui-host-tier-smoke.py")
smoke = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(smoke)

SEEDED = 2  # Amber and Indigo, the two synthetic overview rows
EXACT = {"before_seeding": 0, "after_seeding": SEEDED, "seeded_fixture_vms": SEEDED}
TRIPWIRES = ("runtime_creations", "install_creations", "file_jobs", "backend_calls")
WRONG_COUNTS = [dict(EXACT, after_seeding=count) for count in (0, 1, SEEDED + 1, 2 * SEEDED)]
BEFORE_SEEDING = [dict(EXACT, before_seeding=1), dict(EXACT, before_seeding=1, after_seeding=SEEDED - 1),
                  {"before_seeding": 1, "after_seeding": 0, "seeded_fixture_vms": 0}]


def tripwires(**counts):
    return dict(dict.fromkeys(TRIPWIRES, 0), **counts)


class ModelCreationContracts(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="bridgevm model accounting ")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name).resolve()
        self.captures = self.write_captures(self.root)

    @staticmethod
    def write_captures(directory):
        captures = []
        for name in original.SCREENSHOTS:
            path = directory / (name + ".png")
            path.write_bytes(png())
            captures.append(dict(name=name, file=path.name, sha256=original.digest(path), width=1, height=1))
        return captures

    def report(self, models=EXACT, counts=None, captures=None):
        return dict(schema_version=1, kind="native-app-ui-diagnostic", fixture_data=True, guest_behavior_proof=False,
                    failure=None, screenshots=copy.deepcopy(captures or self.captures),
                    actions=dict.fromkeys(original.ACTIONS, True), model_creations=copy.deepcopy(models),
                    tripwires=tripwires() if counts is None else counts)

    def verify(self, value):
        (self.root / "ui-observations.json").write_text(json.dumps(value))
        return original.verify_observations(self.root)

    def refuses(self, value):
        with self.assertRaises(ValueError):
            self.verify(value)

    def test_all_three_tiers_share_the_one_observation_verifier(self):
        self.assertIs(host.verify_observations, original.verify_observations)
        self.assertIs(v2.verify_observations, original.verify_observations)
        self.assertEqual(original.TIER, "d6-app-ui")

    def test_exactly_one_model_per_seeded_vm_without_backend_calls_is_accepted(self):
        self.assertEqual(self.verify(self.report()), original.digest(self.root / "ui-observations.json"))

    def test_one_or_seeded_plus_one_creations_are_refused(self):
        for models in WRONG_COUNTS:
            with self.subTest(models=models):
                self.refuses(self.report(models))

    def test_a_creation_before_seeding_is_refused(self):
        for models in BEFORE_SEEDING:
            with self.subTest(models=models):
                self.refuses(self.report(models))

    def test_any_backend_call_or_other_tripwire_is_refused(self):
        for key in TRIPWIRES:
            for count in (1, 2, True):
                with self.subTest(key=key, count=count):
                    self.refuses(self.report(counts=tripwires(**{key: count})))

    def test_report_cannot_redefine_the_seeded_count_or_use_inexact_values(self):
        variants = [dict(EXACT, seeded_fixture_vms=1, after_seeding=1), dict(EXACT, seeded_fixture_vms=3, after_seeding=3),
                    {"before_seeding": 0, "after_seeding": 0, "seeded_fixture_vms": 0},
                    dict(EXACT, after_seeding=2.0), dict(EXACT, before_seeding=False), dict(EXACT, after_seeding="2"),
                    {"before_seeding": 0, "after_seeding": SEEDED}, dict(EXACT, total=SEEDED), [0, SEEDED, SEEDED], None]
        for models in variants:
            with self.subTest(models=models):
                self.refuses(self.report(models))
        missing = self.report()
        del missing["model_creations"]
        self.refuses(missing)
        old_success_shape = self.report()
        del old_success_shape["model_creations"]
        old_success_shape["tripwires"] = dict.fromkeys(("model_creations", "runtime_creations", "install_creations", "file_jobs"), 0)
        self.refuses(old_success_shape)
        for counts in (dict(tripwires(), model_creations=SEEDED), {key: 0 for key in TRIPWIRES[:-1]}):
            with self.subTest(counts=counts):
                self.refuses(self.report(counts=counts))

    def test_v1_host_composition_accepts_only_the_exact_count(self):
        helper = smoke.AppUIHostContracts("test_bundle_reconstruction_uses_exact_git_resources_and_unmodified_binary")
        helper.setUp()
        self.addCleanup(helper.doCleanups)
        bundle, observations, launch = helper.prepared()
        paths, _ = helper.write_reports(observations, bundle, launch)
        captures = json.loads((observations / "ui-observations.json").read_text())["screenshots"]

        def run(value):
            (observations / "ui-observations.json").write_text(json.dumps(value))
            completion = json.loads(paths[1].read_text())
            completion["report_sha256"] = original.digest(observations / "ui-observations.json")
            paths[1].write_text(json.dumps(completion))
            return host.verify_result(observations, bundle, helper.binary_hash, launch, 0)

        accepted = run(self.report(captures=captures))
        self.assertEqual(accepted, original.digest(observations / "ui-observations.json"))
        for value in [self.report(models, captures=captures) for models in WRONG_COUNTS + BEFORE_SEEDING] + [
                self.report(counts=tripwires(backend_calls=1), captures=captures)]:
            with self.subTest(models=value["model_creations"], tripwires=value["tripwires"]):
                with self.assertRaises(ValueError):
                    run(value)

    def test_v2_dual_app_composition_accepts_only_the_exact_count(self):
        f = Fixture(self)
        f.write_driver()
        observations = f.private / "host-observations"
        observations.mkdir(mode=0o700)
        captures = self.write_captures(observations)
        owner = f.roles["host"]
        identity = dict(schema_version=1, kind="native-app-ui-host-identity", started_uptime=101.5,
                        **{key: owner[key] for key in ("pid", "bundle_identifier", "bundle_path",
                                                       "executable_path", "executable_sha256")})
        (observations / "host-identity.json").write_text(json.dumps(identity))

        def run(value):
            (observations / "ui-observations.json").write_text(json.dumps(value))
            completion = dict(schema_version=1, kind="native-app-ui-host-completion", pid=owner["pid"], success=True,
                              cleanup_verified=True, report_sha256=original.digest(observations / "ui-observations.json"),
                              failure=None)
            (observations / "host-completion.json").write_text(json.dumps(completion))
            unsigned = lambda private, fields: v2_bundle.verify_bundles(private, fields, signatures=False)
            with mock.patch.object(v2, "verify_bundles", side_effect=unsigned):
                return v2.verify_result(f.private, f.fields, 0)

        report_hash, _ = run(self.report(captures=captures))
        self.assertEqual(report_hash, original.digest(observations / "ui-observations.json"))
        for value in [self.report(models, captures=captures) for models in WRONG_COUNTS + BEFORE_SEEDING] + [
                self.report(counts=tripwires(backend_calls=1), captures=captures)]:
            with self.subTest(models=value["model_creations"], tripwires=value["tripwires"]):
                with self.assertRaises(ValueError):
                    run(value)


if __name__ == "__main__":
    unittest.main(verbosity=2)
