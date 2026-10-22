#!/usr/bin/env python3
"""T17 cleanup forgets each lane's vTPM key, and anything but the product's
exact proof keeps the job tree with its cleanup unverified."""
from __future__ import annotations

import contextlib
import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import re
import secrets
import shutil
import stat
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

ROOT = Path(__file__).resolve().parents[2]
TOOL = ROOT / "scripts/live-gates/t17_lane_key_forget.py"
WRITER = ROOT / "scripts/live-gates/make-windows-product-e2e-request.py"
FAKE_APP = ROOT / "tests/fixtures/fake-vtpm-lifecycle-app.py"
SOURCES = ROOT / "apps/macos/Sources/BridgeVMControl/HvfEngine"
STUB = '#!/bin/sh\nexec /usr/bin/python3 "$(dirname "$0")/../Resources/fake-vtpm-lifecycle-app.py" "$@"\n'
ASSETS = ("app_bundle", "app_executable", "product_helper", "runner", "firmware", "secure_boot_policy",
          "iso", "bundled_vars_seed", "guest_payload", "guest_payload_manifest")
JOB = "key-forget-unit-fixture"


def load(name: str, path: Path):
    sys.path.insert(0, str(path.parent))
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def install_stub_app(app: Path) -> None:
    """Make APP's packaged executable the file-keychain lifecycle stub."""
    for name in ("MacOS", "Resources"):
        (app / "Contents" / name).mkdir(parents=True, exist_ok=True)
    shutil.copyfile(FAKE_APP, app / "Contents/Resources/fake-vtpm-lifecycle-app.py")
    executable = app / "Contents/MacOS/BridgeVMControl"
    executable.write_text(STUB)
    executable.chmod(0o755)


def options(call: list[str]) -> dict[str, str]:
    return dict(zip(call[2::2], call[3::2]))


class LaneKeyForgetTest(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = Path(tempfile.mkdtemp(prefix="t17-key-forget-")).resolve()
        self.addCleanup(shutil.rmtree, self.temporary, ignore_errors=True)
        self.app = self.temporary / "BridgeVMControl.app"
        install_stub_app(self.app)
        self.executable = self.app / "Contents/MacOS/BridgeVMControl"
        self.keychain = self.temporary / "fake-vtpm-keychain"
        self.keychain.mkdir()
        private = self.temporary / "private"
        private.mkdir(mode=0o700)
        self.work = private / f"bridgevm-e2e-{JOB}.{secrets.token_hex(3)}"
        self.work.mkdir(mode=0o700)
        self.verified = private / "verified-inputs.json"
        self.seal("pilot")

    def seal(self, mode: str, digest: str | None = None) -> None:
        assets = {key: {"path": str(self.temporary / key), "sha256": "0" * 64} for key in ASSETS}
        assets["app_bundle"]["path"] = str(self.app)
        assets["app_executable"] = {"path": str(self.executable),
                                    "sha256": digest or hashlib.sha256(self.executable.read_bytes()).hexdigest()}
        self.mode = mode
        self.verified.write_text(json.dumps({"valid": True, "verified": True, "campaign_mode": mode,
                                             "failure_code": "none", "detail": "none", "assets": assets}))

    def lane(self, number: int, state: str = "data", key: bool = True) -> dict:
        root = self.work / f"lane-{number}"
        root.mkdir(mode=0o700)
        subprocess.run([sys.executable, str(WRITER), "--out", str(root / "request.json"), "--verified", str(self.verified),
                        "--job-id", JOB, "--commit", "a" * 40, "--mode", self.mode, "--lane", str(number),
                        "--nonce", secrets.token_hex(32), "--lane-root", str(root)], check=True)
        request = json.loads((root / "request.json").read_text())
        vtpm = Path(request["vtpm_state_path"])
        if state in ("data", "empty"):
            vtpm.mkdir(parents=True)
        if state == "data":
            (vtpm / "tpm2-00.permall").write_bytes(b"encrypted vTPM state")
        if key:
            self.key(request).write_text(secrets.token_hex(32))
        return request

    def key(self, request: dict, suffix: str = "key") -> Path:
        return self.keychain / f"{request['vm_slug']}.{suffix}"

    def forget(self) -> subprocess.CompletedProcess:
        return subprocess.run([sys.executable, str(TOOL), "--work", str(self.work), "--verified", str(self.verified)],
                              capture_output=True, text=True, check=False)

    def calls(self) -> list[list[str]]:
        log = self.keychain / "calls.jsonl"
        return [json.loads(line) for line in log.read_text().splitlines()] if log.exists() else []

    def assert_refused(self, result: subprocess.CompletedProcess, commands: list[str]) -> None:
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertIn("T17 lane vTPM key forget refused", result.stderr)
        self.assertEqual([call[1] for call in self.calls()], commands)

    def assert_forgotten(self, requests: list[dict], scratch: bool = False) -> None:
        calls = self.calls()
        self.assertEqual([call[:2] for call in calls], [["--vtpm-lifecycle", "export"], ["--vtpm-lifecycle", "forget-import"]] * len(requests))
        for request, export, forget in zip(requests, calls[::2], calls[1::2]):
            self.assertEqual(export[2:], forget[2:], "forget-import must reuse the export's identity, state, package and code")
            values = options(export)
            folder = Path(values["--package"]).parent
            self.assertEqual(values["--stable-vm-id"], request["vm_slug"])
            self.assertEqual(folder.parent, Path(request["lane_root"]))
            self.assertTrue(folder.name.startswith("vtpm-key-forget."))
            self.assertEqual(stat.S_IMODE(folder.lstat().st_mode), 0o700)
            self.assertEqual(Path(values["--recovery-code-file"]).parent, folder)
            state = Path(values["--state-dir"])
            self.assertEqual(state, folder / "empty-state" if scratch else Path(request["vtpm_state_path"]))
            self.assertFalse(self.key(request).exists(), "the lane key survived cleanup")

    def test_state_and_key_are_exported_then_forgotten_for_exactly_that_slug(self) -> None:
        request = self.lane(1)
        (Path(request["library_root_path"]) / "Derived/WindowsInstallSources").mkdir(parents=True)
        decoys = [self.keychain / "windows-11.key", self.keychain / f"bridgevm-t17-lane-1-{'0' * 12}.key"]
        for decoy in decoys:
            decoy.write_text("11" * 32)
        result = self.forget()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assert_forgotten([request])
        self.assertTrue(all(decoy.exists() for decoy in decoys), "another VM's key was touched")

    def test_refused_forget_fails_closed_and_keeps_the_key(self) -> None:
        request = self.lane(1)
        self.key(request, "refuse").touch()
        self.assert_refused(self.forget(), ["export", "forget-import"])
        self.assertTrue(self.key(request).exists())

    def test_forget_without_its_exact_line_fails_closed(self) -> None:
        request = self.lane(1)
        self.key(request, "silent").touch()
        self.assert_refused(self.forget(), ["export", "forget-import"])
        self.assertTrue(self.key(request).exists())

    def test_state_with_data_but_no_key_fails_closed(self) -> None:
        self.lane(1, key=False)
        self.assert_refused(self.forget(), ["export"])

    def test_empty_state_with_a_key_is_forgotten(self) -> None:
        request = self.lane(1, state="empty")
        self.assertEqual(self.forget().returncode, 0)
        self.assert_forgotten([request])
        self.assertEqual(os.listdir(request["vtpm_state_path"]), [])

    def test_absent_state_with_a_key_is_forgotten_through_an_empty_scratch_state(self) -> None:
        request = self.lane(1, state="absent")
        self.assertEqual(self.forget().returncode, 0)
        self.assert_forgotten([request], scratch=True)
        self.assertFalse(os.path.lexists(request["library_root_path"]), "cleanup created product library state")

    def test_empty_or_absent_state_without_a_key_has_none_to_forget(self) -> None:
        self.seal("release")
        empty, absent = self.lane(1, state="empty", key=False), self.lane(2, state="absent", key=False)
        result = self.forget()
        self.assertEqual(result.returncode, 0, result.stderr)
        calls = self.calls()
        self.assertEqual([call[1] for call in calls], ["export", "export"])
        self.assertEqual([options(call)["--stable-vm-id"] for call in calls], [empty["vm_slug"], absent["vm_slug"]])

    def test_empty_state_export_failure_without_the_missing_key_message_fails_closed(self) -> None:
        request = self.lane(1, state="empty", key=False)
        self.key(request, "unreadable").touch()
        self.assert_refused(self.forget(), ["export"])

    def test_executable_hash_mismatch_runs_nothing(self) -> None:
        request = self.lane(1)
        self.seal("pilot", digest="0" * 64)
        self.assert_refused(self.forget(), [])
        self.seal("pilot")
        with self.executable.open("a") as executable:
            executable.write("# changed after sealing\n")
        self.assert_refused(self.forget(), [])
        self.assertTrue(self.key(request).exists())

    def test_forged_slug_or_path_runs_nothing(self) -> None:
        request = self.lane(1)
        path = Path(request["lane_root"]) / "request.json"
        original = path.read_text()
        other = "0" * 12
        forgeries = {
            "slug for another nonce": {"vm_slug": f"bridgevm-t17-lane-1-{other}"},
            "product default slug": {"vm_slug": "windows-11"},
            "name for another nonce": {"vm_name": f"BridgeVM T17 Lane 1 {other}"},
            "ordinal of another lane": {"lane": 2},
            "state outside its bundle": {"vtpm_state_path": str(self.temporary / "elsewhere/metadata/vtpm")},
            "state of another slug": {"vtpm_state_path": request["vtpm_state_path"].replace(request["vm_slug"], f"bridgevm-t17-lane-1-{other}")},
            "state with traversal": {"vtpm_state_path": request["vtpm_state_path"] + "/../../../../../lane-1/library"},
            "library elsewhere": {"library_root_path": str(self.temporary / "library")},
            "another lane root": {"lane_root": str(self.work / "lane-2")},
            "another app": {"app_executable_path": "/Applications/BridgeVM.app/Contents/MacOS/BridgeVMControl"},
            "3D injection": {"three_d_injection": True},
            "unknown field": {"extra": True},
        }
        for name, change in forgeries.items():
            with self.subTest(name):
                path.write_text(json.dumps({**json.loads(original), **change}))
                self.assert_refused(self.forget(), [])
        path.write_text(original)
        vtpm = Path(request["vtpm_state_path"])
        shutil.rmtree(vtpm)
        vtpm.symlink_to(self.temporary)
        self.assert_refused(self.forget(), [])
        self.assertTrue(self.key(request).exists())

    def test_three_release_lanes_forget_three_slugs(self) -> None:
        self.seal("release")
        requests = [self.lane(number) for number in (1, 2, 3)]
        self.assertEqual(len({request["vm_slug"] for request in requests}), 3)
        result = self.forget()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assert_forgotten(requests)

    def test_lane_set_must_be_exactly_the_campaign_lanes(self) -> None:
        empty = self.work / "lane-1"
        empty.mkdir(mode=0o700)
        self.assertEqual(self.forget().returncode, 0, "a lane whose request was never written owns no key")
        (empty / "product-app.log").write_text("")
        self.assert_refused(self.forget(), [])
        (empty / "product-app.log").unlink()
        (self.work / "lane-2").mkdir(mode=0o700)
        self.assert_refused(self.forget(), [])
        (self.work / "lane-2").rmdir()
        (self.work / "stray").write_text("")
        self.assert_refused(self.forget(), [])

    def test_another_vm_registration_in_the_lane_library_runs_nothing(self) -> None:
        request = self.lane(1)
        (Path(request["library_root_path"]) / "Derived/WindowsInstallSources").mkdir(parents=True)
        (Path(request["library_root_path"]) / "windows-11").mkdir()
        self.assert_refused(self.forget(), [])
        self.assertTrue(self.key(request).exists())

    def test_a_command_that_does_not_return_fails_closed(self) -> None:
        request = self.lane(1)
        self.key(request, "hang").touch()
        forget = load("t17_lane_key_forget", TOOL)
        with mock.patch.object(forget, "TIMEOUT_SECONDS", 1), contextlib.redirect_stderr(io.StringIO()) as stderr:
            status = forget.main(["--work", str(self.work), "--verified", str(self.verified)])
        self.assertEqual(status, 1)
        self.assertIn("did not finish", stderr.getvalue())
        self.assertTrue(self.key(request).exists())

    def test_product_messages_and_lines_match_the_packaged_sources(self) -> None:
        forget = load("t17_lane_key_forget", TOOL)
        fake = load("fake_vtpm_lifecycle_app", FAKE_APP)
        security = (SOURCES / "VTPMStateSecurity.swift").read_text()
        missing = re.search(r'case \.missingKeyForExistingState:\s*return "([^"]+)"', security).group(1)
        self.assertEqual(forget.MISSING_KEY, "ERROR: " + missing)
        self.assertEqual(fake.MISSING_KEY, missing)
        self.assertIn('Data("ERROR: \\(error.localizedDescription)\\n".utf8)', (SOURCES / "VTPMLifecycleCommand.swift").read_text())
        cleanup = (SOURCES / "VTPMImportCleanupCommand.swift").read_text()
        self.assertIn(f'print("{forget.FORGOT}\\(stableID)")', cleanup)
        self.assertIn(f'"{fake.MISMATCH}"', (SOURCES / "VTPMImportedRecoveryCleanup.swift").read_text())


def augment_synthetic_helper(path: Path) -> None:
    """Store a key for each lane the synthetic product starts, as the product's first start does."""
    path.write_text(path.read_text() + '''
keychain = pathlib.Path(r["app_bundle_path"]).parent / "fake-vtpm-keychain"; keychain.mkdir(exist_ok=True)
(keychain / (r["vm_slug"] + ".key")).write_text(os.urandom(32).hex())
"key-forget-refused" in r["job_id"] and (keychain / (r["vm_slug"] + ".refuse")).write_text("")
''')


def run_tier_fixtures(tier: Path, manifest: Path, temporary: Path) -> None:
    keychain = temporary / "fake-vtpm-keychain"
    cases = (("key-forget-pass-fixture", "completed", "none"), ("key-forget-partial-fixture", "failed", "internal-error"),
             ("key-forget-handoff-fixture", "completed", "none"), ("key-forget-refused-fixture", "cleanup-failed", "cleanup-failed"))
    for job, outcome, failure in cases:
        out = temporary / f"{job}-out"
        destination = temporary / "retained" / job
        command = [str(tier), "--out", str(out), "--input-manifest", str(manifest), "--job-id", job]
        command += ["--retain-import-source", str(destination)] if "handoff" in job else []
        completed = subprocess.run(command, capture_output=True, text=True, check=False)
        private = out / "private"
        receipt = json.loads((out / "receipt.json").read_text())
        slug = "bridgevm-t17-lane-1-" + json.loads((private / "lane-1-result.json").read_text())["nonce"][:12]
        lane = Path(next(line.removeprefix("lane_root=") for line in (private / "lane-1-helper.log").read_text().splitlines()
                         if line.startswith("lane_root=")))
        calls = [call for call in map(json.loads, (keychain / "calls.jsonl").read_text().splitlines())
                 if options(call).get("--stable-vm-id") == slug]
        assert (receipt["outcome"], receipt["failure_code"]) == (outcome, failure), (job, receipt["outcome"], completed.stderr[-1500:])
        assert receipt["pass"] is (outcome == "completed") and receipt["worker_cleanup_verified"] is (outcome != "cleanup-failed"), receipt
        if outcome != "cleanup-failed":
            assert not (keychain / f"{slug}.key").exists(), f"{job} reported verified cleanup but its lane key survived"
            assert not os.path.lexists(lane.parent), f"{job} kept its tree"
        assert [call[1] for call in calls[-2:]] == ["export", "forget-import"] and calls[-2][2:] == calls[-1][2:], (job, calls)
        values = options(calls[-1])
        assert Path(values["--package"]).parent.parent == lane, (job, values)
        assert values["--state-dir"] == str(lane / "library" / slug / "bundle.vmbridge/metadata/vtpm"), (job, values)
        if "handoff" in job:
            assert len(calls) == 3 and calls[0][1] == "export", (job, calls)
            assert Path(options(calls[0])["--package"]).parent.parent == destination.parent, "the T19 handoff export did not precede the forget"
            assert (private / "t19-source-handoff.json").is_file() and not (private / "t19-source-handoff-failed").exists()
        else:
            assert len(calls) == 2, (job, calls)
        if outcome == "cleanup-failed":
            assert "T17 lane vTPM key forget refused" in completed.stderr, completed.stderr[-1500:]
            assert (private / "cleanup-failed").is_file() and (lane / "request.json").is_file(), "a refused forget did not keep the tree"
            assert (keychain / f"{slug}.key").is_file(), "the refused key disappeared"
            shutil.rmtree(lane.parent)
            for suffix in ("key", "refuse"):
                (keychain / f"{slug}.{suffix}").unlink()


if __name__ == "__main__":
    if sys.argv[1:2] == ["--stub-app"]:
        install_stub_app(Path(sys.argv[2]))
    elif sys.argv[1:2] == ["--augment-helper"]:
        augment_synthetic_helper(Path(sys.argv[2]))
    elif sys.argv[1:2] == ["--tier-fixtures"]:
        run_tier_fixtures(Path(sys.argv[2]), Path(sys.argv[3]), Path(sys.argv[4]))
    else:
        unittest.main()
