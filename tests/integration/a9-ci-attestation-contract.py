#!/usr/bin/env python3
"""Offline A9 CI attestation contracts: JSON-alone validation and the gh fetch helper.

A fake `gh` first on PATH serves canned GitHub API responses, so no network or GitHub
account is involved. The green path runs the helper as a separate process; refusals call
its real main() in-process with argv, environment and output captured.
"""
from __future__ import annotations

import contextlib
import importlib.util
import io
import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))
import a9_ci_attestation as CI  # noqa: E402

HELPER = ROOT / "scripts/fetch-a9-ci-attestation.py"
SPEC = importlib.util.spec_from_file_location("a9_ci_fetch_contract", HELPER)
FETCH = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(FETCH)
COMMIT, OTHER = "1a" * 20, "2" * 40
API = f"repos/{CI.REPOSITORY}/actions"
FAKE_GH = """#!/bin/sh
printf '%s\\n' "$*" >> "$FAKE_GH_CALLS"
for last; do :; done
response="$FAKE_GH_ROOT/$(printf '%s' "$last" | tr '/?' '_-')"
if [ "$1" != api ] || [ ! -f "$response" ]; then echo 'gh: Not Found (HTTP 404)' >&2; exit 1; fi
exec cat "$response"
"""


def response_name(path: str) -> str:
    return path.replace("/", "_").replace("?", "-")


def attestation(kind: str, run_id: str) -> dict:
    names = CI.KINDS[kind]
    return {"schema_version": CI.SCHEMA, "repository": CI.REPOSITORY, "workflow_name": names["workflow_name"],
            "workflow_path": names["workflow_path"], "run_id": run_id, "run_attempt": 1, "head_sha": COMMIT,
            "checked_source_sha": COMMIT if names["checked_source"] else "absent", "event": names["events"][0],
            "status": "completed", "conclusion": "success",
            "html_url": f"https://github.com/{CI.REPOSITORY}/actions/runs/{run_id}", "fetched_at": "2026-09-02T00:00:00Z"}


def run_record(run_id: str, kind: str, **changes) -> dict:
    names = CI.KINDS[kind]
    value = {"id": int(run_id), "name": names["workflow_name"], "path": names["workflow_path"], "head_sha": COMMIT,
             "event": names["events"][0], "status": "completed", "conclusion": "success", "run_attempt": 1,
             "html_url": f"https://github.com/{CI.REPOSITORY}/actions/runs/{run_id}",
             "repository": {"full_name": CI.REPOSITORY}, "head_repository": {"full_name": CI.REPOSITORY}}
    value.update(changes)
    return value


def job_log(*shas: str) -> str:
    lines = [f"2026-10-24T04:09:06.9052840Z   SOURCE_SHA: {COMMIT}",
             "2026-10-24T04:09:11.7595350Z \x1b[36;1mprintf 'checked_source_sha=%s\\n' \"$ACTUAL_SHA\"\x1b[0m"]
    lines += [f"2026-10-24T04:09:11.8017810Z checked_source_sha={sha}" for sha in shas]
    return "\n".join(lines + ["2026-10-24T04:48:30.0000000Z PASS: project check"]) + "\n"


class JsonAlone(unittest.TestCase):
    def test_attestations_validate_only_for_their_workflow_and_commit(self):
        for kind, run_id in (("hosted-full", "1001"), ("security-quality", "1002")):
            value = attestation(kind, run_id)
            self.assertIs(CI.validate(value, kind=kind, commit=COMMIT), value)
            self.assertTrue(CI.succeeded(value))
            other = "security-quality" if kind == "hosted-full" else "hosted-full"
            for call in (dict(kind=other, commit=COMMIT), dict(kind=kind, commit=OTHER), dict(kind="ci", commit=COMMIT),
                         dict(kind=kind, commit=COMMIT.upper())):
                with self.subTest(kind=kind, call=call), self.assertRaises(CI.AttestationError):
                    CI.validate(value, **call)
        failed = {**attestation("hosted-full", "1001"), "conclusion": "failure"}
        self.assertFalse(CI.succeeded(CI.validate(failed, kind="hosted-full", commit=COMMIT)))

    def test_malformed_or_unbound_attestations_refuse(self):
        mutations = {
            "hosted-full": [("future_field", "x"), ("schema_version", "bridgevm.a9-ci-run-attestation.v2"),
                            ("repository", "someone/bridgevm"), ("run_id", "0"), ("run_id", "01"), ("run_id", 1001),
                            ("run_attempt", 0), ("run_attempt", True), ("run_attempt", "1"), ("head_sha", COMMIT.upper()),
                            ("checked_source_sha", "absent"), ("checked_source_sha", OTHER), ("event", "push"),
                            ("status", "queued"), ("conclusion", None), ("conclusion", "green"), ("conclusion", ["success"]),
                            ("html_url", f"http://github.com/{CI.REPOSITORY}/actions/runs/1001"),
                            ("html_url", f"https://github.com/{CI.REPOSITORY}/actions/runs/1002"),
                            ("fetched_at", "2026-09-02 00:00:00"), ("fetched_at", "2026-02-30T00:00:00Z")],
            "security-quality": [("checked_source_sha", COMMIT), ("event", "pull_request"), ("event", "schedule"),
                                 ("event", ["push"]), ("workflow_path", ".github/workflows/ci.yml")],
        }
        for kind, changes in mutations.items():
            for field, item in changes:
                value = attestation(kind, "1001" if kind == "hosted-full" else "1002")
                value[field] = item
                with self.subTest(kind=kind, field=field, item=item), self.assertRaises(CI.AttestationError):
                    CI.validate(value, kind=kind, commit=COMMIT)
        missing = attestation("hosted-full", "1001")
        del missing["fetched_at"]
        with self.assertRaises(CI.AttestationError):
            CI.validate(missing, kind="hosted-full", commit=COMMIT)
        with self.assertRaises(CI.AttestationError):
            CI.validate([], kind="hosted-full", commit=COMMIT)


class FetchHelper(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="bridgevm-a9-ci-")
        self.addCleanup(temporary.cleanup)
        root = Path(temporary.name)
        self.api, self.bin, self.out, self.calls = root / "api", root / "bin", root / "out", root / "calls.log"
        self.api.mkdir()
        self.bin.mkdir()
        gh = self.bin / "gh"
        gh.write_text(FAKE_GH)
        gh.chmod(0o755)
        self.serve(f"{API}/runs/1001", run_record("1001", "hosted-full"))
        self.serve(f"{API}/runs/1001/jobs?per_page=100", {"total_count": 2, "jobs": [
            {"id": 2001, "name": "scripts/check-project.sh", "status": "completed", "conclusion": "success"},
            {"id": 2002, "name": "a19-exfat-publication", "status": "completed", "conclusion": "skipped"}]})
        self.serve(f"{API}/jobs/2001/logs", job_log(COMMIT))
        self.serve(f"{API}/runs/1002", run_record("1002", "security-quality"))

    def serve(self, path: str, value: object) -> None:
        (self.api / response_name(path)).write_text(value if isinstance(value, str) else json.dumps(value))

    def fetch(self, *, commit: str = COMMIT, hosted: str = "1001", security: str = "1002", path: str | None = None,
              process: bool = False):
        environment = {"PATH": path or f"{self.bin}{os.pathsep}{os.environ['PATH']}",
                       "FAKE_GH_ROOT": str(self.api), "FAKE_GH_CALLS": str(self.calls)}
        arguments = ["--commit", commit, "--hosted-full-run-id", hosted, "--security-quality-run-id", security,
                     "--out-dir", str(self.out)]
        if process:
            return subprocess.run([sys.executable, str(HELPER), *arguments], env={**os.environ, **environment},
                                  capture_output=True, text=True, timeout=60)
        stdout, stderr, saved_argv, saved_environment = io.StringIO(), io.StringIO(), sys.argv, dict(os.environ)
        sys.argv = [str(HELPER), *arguments]
        os.environ.update(environment)
        try:
            with contextlib.redirect_stdout(stdout), contextlib.redirect_stderr(stderr):
                try:
                    code = FETCH.main()
                except SystemExit as exit:
                    code = exit.code if isinstance(exit.code, int) else 1
        finally:
            sys.argv = saved_argv
            os.environ.clear()
            os.environ.update(saved_environment)
        return subprocess.CompletedProcess(arguments, code, stdout.getvalue(), stderr.getvalue())

    def written(self) -> list[str]:
        return sorted(path.name for path in self.out.iterdir()) if self.out.exists() else []

    def test_green_runs_at_the_commit_produce_verifiable_attestations(self):
        result = self.fetch(process=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.written(), ["hosted-full-ci-attestation.json", "security-quality-ci-attestation.json"])
        for kind, run_id in (("hosted-full", "1001"), ("security-quality", "1002")):
            value = json.loads((self.out / f"{kind}-ci-attestation.json").read_text())
            CI.validate(value, kind=kind, commit=COMMIT)
            self.assertEqual({**value, "fetched_at": "fixed"}, {**attestation(kind, run_id), "fetched_at": "fixed"})
        self.assertEqual(result.stdout.count("PASS: "), 2)
        calls = self.calls.read_text().splitlines()
        self.assertIn(f"api --allow-escape-sequences {API}/jobs/2001/logs", calls)
        self.assertEqual(sum("--allow-escape-sequences" in call for call in calls), 1)

    def test_runs_that_do_not_attest_the_commit_refuse_and_write_nothing(self):
        jobs = f"{API}/runs/1001/jobs?per_page=100"
        cases = {
            "hosted run at another commit": (f"{API}/runs/1001", run_record("1001", "hosted-full", head_sha=OTHER), "head_sha"),
            "security run at another commit": (f"{API}/runs/1002", run_record("1002", "security-quality", head_sha=OTHER), "head_sha"),
            "hosted run failed": (f"{API}/runs/1001", run_record("1001", "hosted-full", conclusion="failure"), "concluded 'failure'"),
            "security run cancelled": (f"{API}/runs/1002", run_record("1002", "security-quality", conclusion="cancelled"),
                                       "concluded 'cancelled'"),
            "hosted run in progress": (f"{API}/runs/1001", run_record("1001", "hosted-full", status="in_progress", conclusion=None),
                                       "not completed"),
            "hosted id names a security run": (f"{API}/runs/1001", run_record("1001", "security-quality"), "not 'Hosted full project check'"),
            "renamed workflow file": (f"{API}/runs/1002", run_record("1002", "security-quality", path=".github/workflows/ci.yml"),
                                      "not 'Security and quality'"),
            "fork code": (f"{API}/runs/1002", run_record("1002", "security-quality", head_repository={"full_name": "fork/bridgevm"}),
                          "does not run"),
            "pull_request security run": (f"{API}/runs/1002", run_record("1002", "security-quality", event="pull_request"),
                                          "event 'pull_request'"),
            "gh returns another run": (f"{API}/runs/1002", run_record("9999", "security-quality"), "gh returned run"),
            "malformed API JSON": (f"{API}/runs/1002", "not json", "malformed JSON"),
            "log checked another source": (f"{API}/jobs/2001/logs", job_log(OTHER), "checked_source_sha"),
            "log lacks the source line": (f"{API}/jobs/2001/logs", job_log(), "has 0 checked_source_sha lines"),
            "log repeats the source line": (f"{API}/jobs/2001/logs", job_log(COMMIT, COMMIT), "has 2 checked_source_sha lines"),
            "check job failed": (jobs, {"jobs": [{"id": 2001, "name": "scripts/check-project.sh", "status": "completed",
                                                  "conclusion": "failure"}]}, "did not complete successfully"),
            "check job missing": (jobs, {"jobs": []}, "exactly one scripts/check-project.sh job"),
        }
        for name, (path, value, fragment) in cases.items():
            with self.subTest(name):
                saved = (self.api / response_name(path)).read_bytes()
                self.serve(path, value)
                result = self.fetch()
                self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
                self.assertTrue(result.stderr.startswith("refusing A9 CI attestation: "), result.stderr)
                self.assertIn(fragment, result.stderr)
                self.assertEqual(self.written(), [])
                (self.api / response_name(path)).write_bytes(saved)
        for name, arguments, fragment in (("unknown run", {"security": "1003"}, "HTTP 404"),
                                          ("non-numeric run id", {"hosted": "abc"}, "not a decimal GitHub run id"),
                                          ("gh absent", {"path": str(self.api)}, "gh is not installed")):
            with self.subTest(name):
                result = self.fetch(**arguments)
                self.assertEqual(result.returncode, 1, result.stderr)
                self.assertIn(fragment, result.stderr)
                self.assertEqual(self.written(), [])
        self.assertEqual(self.fetch(commit="ABC").returncode, 2)

    def test_existing_attestation_is_never_replaced(self):
        self.out.mkdir()
        kept = self.out / "security-quality-ci-attestation.json"
        kept.write_text("keep\n")
        result = self.fetch()
        self.assertEqual(result.returncode, 1, result.stderr)
        self.assertIn("refusing to replace", result.stderr)
        self.assertEqual(self.written(), ["security-quality-ci-attestation.json"])
        self.assertEqual(kept.read_text(), "keep\n")
        self.assertFalse(self.calls.exists(), "gh was called before the output check")


if __name__ == "__main__":
    unittest.main(verbosity=1)
