#!/usr/bin/env python3
"""Offline A9 release-claim contracts through the real build and verify commands.

Synthetic receipts and attestations only: no network, live queue, app, guest media or
capability-registry write is involved. Most cases call the real main() in-process with
its argv and output captured; the positive journey also runs as a separate process.
"""
from __future__ import annotations

import contextlib
import copy
import hashlib
import importlib.util
import io
import json
import os
import re
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SCRIPT = ROOT / "scripts/a9-release-claim.py"
REGISTRY = ROOT / "capabilities/windows-hvf.json"
SPEC = importlib.util.spec_from_file_location("a9_release_claim_contract", SCRIPT)
CLAIM = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CLAIM)
COMMIT, OTHER = CLAIM.fixtures()["commit"], "2" * 40
INPUTS = ("t17", "t19", "hosted_full", "security_quality")
FLAGS = {"t17": "--t17", "t19": "--t19", "hosted_full": "--hosted-full-ci",
         "security_quality": "--security-quality-ci", "clean": "--clean-host-attestation"}


def failed_t17() -> dict:
    value = CLAIM.t17_receipt()
    value.update(passes=2, failures=1, outcome="failed", failure_code="integration-failed", **{"pass": False})
    value.update({field: 2 for field in CLAIM.T17.STAGE_FIELDS[-4:]})
    return value


def short_t19() -> dict:
    value = CLAIM.t19_receipt()
    value.update(run_count=2, passes=2, outcome="canceled", failure_code="canceled", **{"pass": False})
    value.update({field: 2 for field in CLAIM.T19.STAGE_FIELDS})
    return value


def run(*arguments: str, process: bool = False) -> subprocess.CompletedProcess:
    if process:
        return subprocess.run([sys.executable, str(SCRIPT), *arguments], capture_output=True, text=True, timeout=60)
    stdout, stderr, saved = io.StringIO(), io.StringIO(), sys.argv
    sys.argv = [str(SCRIPT), *arguments]
    try:
        with contextlib.redirect_stdout(stdout), contextlib.redirect_stderr(stderr):
            try:
                code = CLAIM.main()
            except SystemExit as exit:
                code = exit.code if isinstance(exit.code, int) else 1
    finally:
        sys.argv = saved
    return subprocess.CompletedProcess(arguments, code, stdout.getvalue(), stderr.getvalue())


class Case:
    """One directory of input files written exactly as the CLI will read them."""

    def __init__(self, test: unittest.TestCase, values: dict, *, clean: bool = True):
        test.counter += 1
        self.directory = test.root / f"case-{test.counter}"
        self.directory.mkdir()
        self.paths = {key: self.write(key, values[key]) for key in INPUTS + (("clean",) if clean else ())}
        self.sha = hashlib.sha256(self.paths["clean"].read_bytes()).hexdigest() if clean else None
        self.out = self.directory / "claim.json"

    def write(self, name: str, value: object) -> Path:
        path = self.directory / f"{name}.json"
        path.write_bytes(CLAIM.encode(value))
        return path

    def inputs(self, omit: tuple = ()) -> list[str]:
        return [str(item) for key, path in self.paths.items() if key not in omit for item in (FLAGS[key], path)]

    def build(self, *extra: str, commit: str = COMMIT, omit: tuple = (), sha: str | None = "auto", process: bool = False):
        recorded = self.sha if sha == "auto" else sha
        record = ["--clean-host-attestation-sha256", recorded] if recorded else []
        return run("build", "--commit", commit, *self.inputs(omit), *record, "--out", str(self.out), *extra, process=process)

    def verify(self, *extra: str, receipt: Path | None = None, omit: tuple = (), process: bool = False):
        return run("verify", str(receipt or self.out), *self.inputs(omit), *extra, process=process)


class ReleaseClaim(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.registry = hashlib.sha256(REGISTRY.read_bytes()).hexdigest()

    @classmethod
    def tearDownClass(cls):
        assert hashlib.sha256(REGISTRY.read_bytes()).hexdigest() == cls.registry, "capability registry changed"

    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="bridgevm-a9-claim-")
        self.addCleanup(temporary.cleanup)
        self.root, self.counter = Path(temporary.name), 0

    def mutated(self, mutate) -> dict:
        values = copy.deepcopy(CLAIM.fixtures())
        mutate(values)
        return values

    def refused(self, result: subprocess.CompletedProcess, fragment: str, case: Case | None = None) -> None:
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertTrue(result.stderr.startswith("FAIL: A9 release claim: "), result.stderr)
        self.assertIn(fragment, result.stderr)
        if case is not None:
            self.assertFalse(case.out.exists(), "a refused build wrote a receipt")

    def eligible_case(self, process: bool = False) -> Case:
        case = Case(self, CLAIM.fixtures())
        result = case.build("--require-claim-eligible", process=process)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("; claim-eligible", result.stdout)
        return case

    def test_complete_consistent_inputs_are_claim_eligible(self):
        case = self.eligible_case(process=True)
        receipt = json.loads(case.out.read_text())
        self.assertEqual((receipt["claim_eligible"], receipt["unmet_conditions"], receipt["clean_machine"]), (True, [], True))
        self.assertEqual((receipt["criterion_pass"], receipt["capability_promotion"], receipt["three_d_injection"]), (False, False, False))
        self.assertEqual(receipt["journeys"]["t17"]["receipt_sha256"], hashlib.sha256(case.paths["t17"].read_bytes()).hexdigest())
        self.assertEqual(receipt["clean_host"]["attestation_sha256"], case.sha)
        result = case.verify("--require-claim-eligible", "--expected-commit", COMMIT, process=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("verified", result.stdout)
        self.refused(case.verify("--expected-commit", OTHER, process=True), "is not the expected")
        again = Case(self, CLAIM.fixtures())
        self.assertEqual(again.build().returncode, 0)
        self.assertEqual(again.out.read_bytes(), case.out.read_bytes(), "the receipt is not a pure function of its inputs")
        self.refused(case.build(), "File exists")
        claimed = CLAIM.T17._fixture("release")  # a T17 receipt that already cites CI and a clean machine
        self.refused(Case(self, self.mutated(lambda v: v.update(t17=claimed))).build(), "is not attested run 1001")
        claimed.update(hosted_ci_run_id="1001", security_ci_run_id="1002")
        self.assertEqual(Case(self, self.mutated(lambda v: v.update(t17=claimed))).build("--require-claim-eligible").returncode, 0)

    def test_contradictions_refuse_without_a_receipt(self):
        def journey_commit(name):
            return lambda v: v[name].update(commit=OTHER, tested_commit=OTHER, hosted_ci_commit=OTHER)
        cases = {
            "T19 at another commit": (journey_commit("t19"), "T19 receipt refused by its existing verifier"),
            "T17 at another commit": (journey_commit("t17"), "T17 receipt refused by its existing verifier"),
            "hosted CI at another commit": (lambda v: v["hosted_full"].update(head_sha=OTHER), "hosted-full CI attestation head_sha"),
            "security CI at another commit": (lambda v: v["security_quality"].update(head_sha=OTHER), "security-quality CI attestation head_sha"),
            "hosted CI checked another source": (lambda v: v["hosted_full"].update(checked_source_sha=OTHER), "checked_source_sha"),
            "security run in the hosted slot": (lambda v: v.update(hosted_full=v["security_quality"]), "workflow_name"),
            "hosted run in the security slot": (lambda v: v.update(security_quality=v["hosted_full"]), "workflow_name"),
            "renamed workflow": (lambda v: v["hosted_full"].update(workflow_name="Hosted full project check copy"), "workflow_name"),
            "other workflow file": (lambda v: v["security_quality"].update(workflow_path=".github/workflows/ci.yml"), "workflow_path"),
            "pull_request security run": (lambda v: v["security_quality"].update(event="pull_request"), "event 'pull_request'"),
            "incomplete CI run": (lambda v: v["hosted_full"].update(status="in_progress"), "is not completed"),
            "CI URL for another run": (lambda v: v["hosted_full"].update(html_url=v["security_quality"]["html_url"]), "html_url"),
            "journey cites another CI run": (lambda v: v["t19"].update(security_ci_run_id="999"), "is not attested run 1002"),
            "journey CI commit differs": (lambda v: v["t17"].update(hosted_ci_commit=OTHER), "hosted_ci_commit"),
            "different app artifacts": (lambda v: v["t19"].update(app_artifact_sha256="cd" * 32), "app_artifact_sha256"),
            "different hosts": (lambda v: v["t19"].update(host_model="Mac17,9"), "host_model"),
            "3D injection": (lambda v: v["t19"].update(three_d_injection=True), "T19 receipt refused"),
            "journey claims the criterion": (lambda v: v["t17"].update(criterion_pass=True), "T17 receipt refused"),
            "apple-development signing": (lambda v: v["t17"].update(artifact_signing_class="apple-development"), "not recognized"),
            "tampered run totals": (lambda v: v["t17"].update(passes=2), "run totals are inconsistent"),
            "T17 receipt in both slots": (lambda v: v.update(t19=v["t17"]), "T19 receipt refused"),
            "T19 receipt in both slots": (lambda v: v.update(t17=v["t19"]), "T17 receipt refused"),
            "attestation for another commit": (lambda v: v["clean"].update(commit=OTHER), "names commit"),
            "attestation for another host": (lambda v: v["clean"].update(host_model="Mac17,9"), "clean-host attestation host_model"),
            "attestation for another macOS": (lambda v: v["clean"].update(macos_version="26.6"), "clean-host attestation macos_version"),
            "attestation after the campaign began": (lambda v: v["clean"].update(attested_at="2026-09-01T00:00:01Z"), "postdates"),
            "attestation with unknown fields": (lambda v: v["clean"].update(witness="x"), "missing or unknown fields"),
            "attestation without a definition": (lambda v: v["clean"].update(clean_machine_definition=""), "clean_machine_definition"),
        }
        for name, (mutate, fragment) in cases.items():
            with self.subTest(name):
                case = Case(self, self.mutated(mutate))
                self.refused(case.build(), fragment, case)
        case = Case(self, CLAIM.fixtures())
        self.refused(case.build(commit=OTHER), "T17 receipt refused", case)
        self.refused(case.build(sha="0" * 64), "unbound", case)
        self.refused(case.build(sha="ABC"), "64 lowercase hex", case)
        self.refused(case.build(sha=None), "supplied together", case)
        bare = Case(self, CLAIM.fixtures(), clean=False)
        self.refused(bare.build(sha=case.sha), "supplied together", bare)
        hostile = Case(self, CLAIM.fixtures())
        hostile.paths["t17"].write_text("[" * 500000 + "]" * 500000)  # RecursionError or a non-object
        self.refused(hostile.build(), "T17 receipt", hostile)
        hostile.paths["t17"].unlink()
        os.mkfifo(hostile.paths["t17"])
        self.refused(hostile.build(process=True), "is not a regular file", hostile)

    def test_one_journey_alone_cannot_build(self):
        for missing in ("t17", "t19"):
            with self.subTest(missing):
                case = Case(self, CLAIM.fixtures())
                result = case.build(omit=(missing,), process=missing == "t19")
                self.assertEqual(result.returncode, 2, result.stderr)
                self.assertIn(f"--{missing}", result.stderr)
                self.assertFalse(case.out.exists())

    def test_consistent_but_unproven_inputs_are_not_claim_eligible(self):
        cases = {
            "T17 pilot": (lambda v: v.update(t17=CLAIM.t17_receipt("pilot")), ["t17-campaign-not-release", "t17-not-three-of-three-passing"]),
            "T19 pilot": (lambda v: v.update(t19=CLAIM.t19_receipt("pilot")), ["t19-campaign-not-release", "t19-not-three-of-three-passing"]),
            "T17 two of three": (lambda v: v.update(t17=failed_t17()), ["t17-not-three-of-three-passing"]),
            "T19 two runs": (lambda v: v.update(t19=short_t19()), ["t19-not-three-of-three-passing"]),
            "T17 ad-hoc signed": (lambda v: v["t17"].update(artifact_signing_class="development-ad-hoc"), ["t17-signing-not-developer-id-notarized"]),
            "T19 Apple Development signed": (lambda v: v["t19"].update(artifact_signing_class="development-signed"), ["t19-signing-not-developer-id-notarized"]),
            "T17 unverified signature": (lambda v: v["t17"].update(artifact_signing_class="unverified"), ["t17-signing-not-developer-id-notarized"]),
            "hosted CI failed": (lambda v: v["hosted_full"].update(conclusion="failure"), ["hosted-full-ci-not-success"]),
            "hosted CI timed out": (lambda v: v["hosted_full"].update(conclusion="timed_out"), ["hosted-full-ci-not-success"]),
            "security CI cancelled": (lambda v: v["security_quality"].update(conclusion="cancelled"), ["security-quality-ci-not-success"]),
            "no clean-host attestation": (None, ["clean-host-attestation-absent"]),
        }
        for name, (mutate, expected) in cases.items():
            with self.subTest(name):
                case = Case(self, self.mutated(mutate or (lambda v: None)), clean=mutate is not None)
                self.refused(case.build("--require-claim-eligible"), "not claim-eligible", case)
                result = case.build()
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertIn("not claim-eligible", result.stdout)
                receipt = json.loads(case.out.read_text())
                self.assertEqual((receipt["claim_eligible"], receipt["unmet_conditions"]), (False, expected))
                self.assertIs(receipt["clean_machine"], mutate is not None)
                self.assertEqual(case.verify().returncode, 0)
                self.refused(case.verify("--require-claim-eligible"), "not claim-eligible")

    def test_verify_refuses_tampered_inputs_and_receipts(self):
        case = self.eligible_case()
        original = json.loads(case.out.read_text())

        def edited(name: str, change) -> Path:
            value = copy.deepcopy(original)
            change(value)
            path = case.directory / f"{name}.claim.json"
            path.write_bytes(CLAIM.encode(value))
            return path
        receipts = {
            "receipt hash": (lambda v: v["journeys"]["t19"].update(receipt_sha256="0" * 64), "receipt.journeys.t19.receipt_sha256"),
            "claim flag": (lambda v: v.update(claim_eligible=False), "fixed release rule"),
            "criterion": (lambda v: v.update(criterion_pass=True), "criterion_pass must be False"),
            "promotion": (lambda v: v.update(capability_promotion=True), "capability_promotion must be False"),
            "clean machine": (lambda v: v.update(clean_machine=False), "clean_machine"),
            "unmet list": (lambda v: v.update(unmet_conditions=["clean-host-attestation-absent"]), "unmet_conditions"),
            "extra field": (lambda v: v.update(release="v1"), "missing or unknown fields"),
            "CI run": (lambda v: v["ci"]["hosted_full"].update(run_id="1003"), "receipt.ci.hosted_full.run_id"),
            "pilot rewrite": (lambda v: v["journeys"]["t17"].update(campaign_mode="pilot", expected_runs=1, run_count=1, passes=1),
                              "unmet_conditions must be"),
        }
        for name, (change, fragment) in receipts.items():
            with self.subTest(name):
                self.refused(case.verify(receipt=edited(name, change)), fragment)
        duplicate = case.directory / "duplicate.claim.json"
        duplicate.write_text(case.out.read_text().replace('"claim_eligible": true', '"claim_eligible": true, "claim_eligible": true'))
        self.refused(case.verify(receipt=duplicate), "duplicate field")
        link = case.directory / "link.claim.json"
        link.symlink_to(case.out)
        self.refused(case.verify(receipt=link), "regular non-symlink")
        self.refused(case.verify(omit=("clean",)), "supply it")
        fixtures = CLAIM.fixtures()
        replacements = {
            "t17": (lambda path: path.write_text(json.dumps(json.loads(path.read_text()))), "receipt.journeys.t17.receipt_sha256"),
            "t19": (lambda path: path.write_bytes(CLAIM.encode({**CLAIM.t19_receipt(), "job_id": "t19-other"})), "receipt.journeys.t19"),
            "hosted_full": (lambda path: path.write_bytes(CLAIM.encode({**fixtures["hosted_full"], "fetched_at": "2026-09-03T00:00:00Z"})),
                            "receipt.ci.hosted_full.attestation_sha256"),
            "clean": (lambda path: path.write_bytes(CLAIM.encode({**fixtures["clean"], "attested_by": "someone-else"})), "unbound"),
        }
        for name, (replace, fragment) in replacements.items():
            with self.subTest(f"replaced {name}"):
                saved = case.paths[name].read_bytes()
                replace(case.paths[name])
                self.refused(case.verify(), fragment)
                case.paths[name].write_bytes(saved)
        self.assertEqual(case.verify("--require-claim-eligible").returncode, 0)
        bare = Case(self, CLAIM.fixtures(), clean=False)
        self.assertEqual(bare.build().returncode, 0)
        bare.paths["clean"] = case.paths["clean"]
        self.refused(bare.verify(), "does not match the receipt's binding")

    def test_schemas_match_the_validators(self):
        def schema(name: str) -> dict:
            return json.loads((ROOT / "schemas" / f"a9-{name}-v1.json").read_text())
        claim, ci, clean = (schema(name) for name in ("release-claim", "ci-run-attestation", "clean-host-attestation"))
        self.assertEqual(set(claim["required"]), CLAIM.FIELDS)
        self.assertEqual(set(claim["properties"]), CLAIM.FIELDS)
        self.assertEqual(set(claim["$defs"]["journey"]["required"]), CLAIM.JOURNEY_FIELDS)
        self.assertEqual(set(claim["$defs"]["ci_run"]["required"]), CLAIM.CI_FIELDS)
        self.assertEqual(set(claim["properties"]["clean_host"]["required"]), CLAIM.CLEAN_FIELDS)
        self.assertEqual(tuple(claim["properties"]["unmet_conditions"]["items"]["enum"]), CLAIM.CONDITIONS)
        self.assertEqual(set(claim["$defs"]["ci_run"]["properties"]["conclusion"]["enum"]), CLAIM.CI.CONCLUSIONS)
        self.assertEqual(claim["properties"]["schema_version"]["const"], CLAIM.SCHEMA)
        self.assertEqual(set(ci["required"]), CLAIM.CI.FIELDS)
        self.assertEqual(set(ci["properties"]["conclusion"]["enum"]), CLAIM.CI.CONCLUSIONS)
        self.assertEqual(set(ci["properties"]["event"]["enum"]), {e for kind in CLAIM.CI.KINDS.values() for e in kind["events"]})
        self.assertEqual(set(clean["required"]), CLAIM.CLEAN_HOST_FIELDS)
        self.assertEqual(clean["properties"]["schema_version"]["const"], CLAIM.CLEAN_HOST_SCHEMA)
        values = CLAIM.fixtures()
        for name, document, value in (("clean-host", clean, values["clean"]), ("CI", ci, values["hosted_full"])):
            for field, rule in document["properties"].items():
                if "pattern" in rule:
                    self.assertRegex(str(value[field]), re.compile(rule["pattern"]), f"{name} {field}")

    def test_self_test_and_registry_boundary(self):
        result = subprocess.run([sys.executable, str(SCRIPT), "--self-test"], capture_output=True, text=True, timeout=60)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("PASS: A9 release claim builder and verifier", result.stdout)
        for path in (SCRIPT, ROOT / "scripts/a9_ci_attestation.py", ROOT / "scripts/fetch-a9-ci-attestation.py"):
            self.assertNotIn("windows-hvf.json", path.read_text(), f"{path.name} names the capability registry")


if __name__ == "__main__":
    unittest.main(verbosity=1)
