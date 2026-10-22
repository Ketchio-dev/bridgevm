#!/usr/bin/env python3
"""T19 cleanup accounts for every lane's imported vTPM key through the sealed app, whatever the helper did: each
candidate must be forgotten or shown absent in the product's exact words, from the lane's sealed inputs, or the
tier keeps the tree with its cleanup unverified."""
from __future__ import annotations

import contextlib
import importlib.util
import io
import json
import os
from pathlib import Path
import re
import secrets
import shutil
import subprocess
import sys
import tempfile
import time
import unittest
from unittest import mock

ROOT = Path(__file__).resolve().parents[2]
TOOL = ROOT / "scripts/live-gates/t19_lane_key_forget.py"
MANIFEST = ROOT / "scripts/live-gates/windows-import-product-e2e-manifest.py"
REQUEST = ROOT / "scripts/live-gates/make-windows-import-product-e2e-request.py"
FAKE_APP = ROOT / "tests/fixtures/fake-vtpm-lifecycle-app.py"
SWIFT = ROOT / "apps/macos/Sources"
STUB = '#!/bin/sh\nexec /usr/bin/python3 "$(dirname "$0")/../Resources/fake-vtpm-lifecycle-app.py" "$@"\n'
HELPER = "Contents/Helpers/BridgeVMProductE2E.app/Contents/MacOS/BridgeVMProductE2E"
RUNNER = "Contents/Resources/target/release/hvf-runner"
SOURCE_VM = "bridgevm-t17-lane-1-0123456789ab"  # the VM whose key the T19 source package seals
JOB = "t19-key-forget-unit"
MISSING = re.search(r'case \.missingKeyForExistingState:\s*return "([^"]+)"',
                    (SWIFT / "BridgeVMControl/HvfEngine/VTPMStateSecurity.swift").read_text()).group(1)


def load(name: str, path: Path):
    sys.path.insert(0, str(path.parent))
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def install_stub_app(app: Path) -> Path:
    """Make APP's packaged executable the file-keychain vTPM lifecycle stub; return that executable."""
    for name in ("MacOS", "Resources"):
        (app / "Contents" / name).mkdir(parents=True, exist_ok=True)
    shutil.copyfile(FAKE_APP, app / "Contents/Resources/fake-vtpm-lifecycle-app.py")
    executable = app / "Contents/MacOS/BridgeVMControl"
    executable.write_text(STUB)
    executable.chmod(0o755)
    return executable


def export_source(app: Path, folder: Path) -> str:
    """Seal SOURCE_VM's key into FOLDER's recovery package and code as the product's export does; return the key."""
    keychain = app.parent / "fake-vtpm-keychain"
    keychain.mkdir(exist_ok=True)
    key = secrets.token_hex(32)
    (keychain / f"{SOURCE_VM}.key").write_text(key)
    subprocess.run([str(app / "Contents/MacOS/BridgeVMControl"), "--vtpm-lifecycle", "export", "--stable-vm-id", SOURCE_VM,
                    "--state-dir", str(folder / "vtpm"), "--package", str(folder / "vtpm-recovery.json"),
                    "--recovery-code-file", str(folder / "vtpm-recovery-code.txt")], check=True, capture_output=True)
    return key


def forget_calls(keychain: Path) -> list[dict[str, str]]:
    log = keychain / "calls.jsonl"
    calls = [json.loads(line) for line in log.read_text().splitlines()] if log.exists() else []
    return [dict(zip(call[2::2], call[3::2])) for call in calls if call[:2] == ["--vtpm-lifecycle", "forget-import"]]


class LaneKeyForgetTest(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = Path(tempfile.mkdtemp(prefix="t19-key-forget-")).resolve()
        self.addCleanup(shutil.rmtree, self.temporary, ignore_errors=True)
        self.addCleanup(subprocess.run, ["chmod", "-R", "u+rwx", str(self.temporary)], check=False)
        self.app, self.source, self.keychain = self.temporary / "BridgeVM.app", self.temporary / "source", self.temporary / "fake-vtpm-keychain"
        self.executable = install_stub_app(self.app)
        for relative in (HELPER, RUNNER):
            (self.app / relative).parent.mkdir(parents=True, exist_ok=True)
            (self.app / relative).write_text("#!/bin/sh\nexit 0\n")
        (self.source / "vtpm").mkdir(parents=True)
        (self.source / "windows.raw").write_bytes(b"installed disk")
        with open(self.source / "vars.fd", "wb") as variables:
            variables.truncate(64 * 1024 * 1024)
        (self.source / "vtpm/tpm2-00.permall").write_bytes(b"sealed source vTPM state")
        (self.source / "vtpm/.lock").write_bytes(b"")
        self.key = export_source(self.app, self.source)
        private = self.temporary / "private"
        private.mkdir(mode=0o700)
        self.work = private / f"bridgevm-import-e2e-{JOB}.{secrets.token_hex(3)}"
        self.work.mkdir(mode=0o700)
        self.verified = private / "verified-inputs.json"
        self.seal("pilot")

    def seal(self, mode: str | None = None) -> None:
        """Authenticate the app and source through the T19 manifest, as the tier does before any lane."""
        self.mode = mode or self.mode
        hashes = load("t19_import_manifest", MANIFEST)
        assets = {"app_bundle": self.app, "app_executable": self.executable, "product_helper": self.app / HELPER,
                  "runner": self.app / RUNNER, "source_disk": self.source / "windows.raw", "source_vars": self.source / "vars.fd",
                  "source_vtpm": self.source / "vtpm", "source_vtpm_package": self.source / "vtpm-recovery.json",
                  "source_vtpm_code": self.source / "vtpm-recovery-code.txt"}
        rows = [f"campaign_mode\t{self.mode}"] + [
            f"{key}\t{path}\t{hashes.tree_hash(path, allow_symlinks=key == 'app_bundle') if path.is_dir() else hashes.file_hash(path)}"
            for key, path in assets.items()]
        manifest = self.temporary / "input-manifest.tsv"
        manifest.write_text("\n".join(rows) + "\n")
        self.verified.unlink(missing_ok=True)
        subprocess.run([sys.executable, str(MANIFEST), "--manifest", str(manifest), "--out", str(self.verified)], check=True)

    def lane(self, number: int, booted: bool = True) -> dict:
        """Prepare a lane as the tier does and, when booted, give it the product's registration and rewritten state."""
        root, inputs = self.work / f"lane-{number}", self.work / f"lane-{number}/inputs"
        root.mkdir(mode=0o700)
        (inputs / "vtpm").mkdir(mode=0o700, parents=True)
        for name in ("windows.raw", "vars.fd", "vtpm-recovery.json", "vtpm-recovery-code.txt"):
            shutil.copy2(self.source / name, inputs / name)
        for item in (self.source / "vtpm").iterdir():
            shutil.copy2(item, inputs / "vtpm" / item.name)
        subprocess.run(["chmod", "-R", "a-w", str(inputs)], check=True)
        subprocess.run([sys.executable, str(REQUEST), "--out", str(root / "request.json"), "--verified", str(self.verified),
                        "--job-id", JOB, "--commit", "a" * 40, "--mode", self.mode, "--lane", str(number),
                        "--nonce", secrets.token_hex(32), "--lane-root", str(root)], check=True)
        request = json.loads((root / "request.json").read_text())
        if booted:
            Path(request["vtpm_state_path"]).mkdir(parents=True)
            (Path(request["vtpm_state_path"]) / "tpm2-00.permall").write_bytes(b"state rewritten by the guest boots")
            (Path(request["library_root_path"]) / request["vm_slug"] / "vm.json").write_text("{}\n")
        return request

    def imported(self, *names: str, marker: str = "key") -> None:
        for name in names:
            (self.keychain / f"{name}.{marker}").write_text(self.key if marker == "key" else "")

    def held(self, name: str) -> bool:
        return (self.keychain / f"{name}.key").exists()

    def calls(self) -> list[dict[str, str]]:
        return forget_calls(self.keychain)

    def clear_calls(self) -> None:
        (self.keychain / "calls.jsonl").unlink(missing_ok=True)

    def script_app(self, body: str) -> None:
        """Replace the packaged executable with a stub that logs its call like the fake app, then runs BODY."""
        self.executable.write_text(
            "#!/usr/bin/python3\nimport json, sys\n"
            f"open({str(self.keychain / 'calls.jsonl')!r}, 'a').write(json.dumps(sys.argv[1:]) + '\\n')\n"
            f"slug, missing = sys.argv[4], {'ERROR: ' + MISSING!r}\n{body}\n")
        self.seal()

    def forget(self) -> subprocess.CompletedProcess:
        result = subprocess.run([sys.executable, str(TOOL), "--work", str(self.work), "--verified", str(self.verified)],
                                capture_output=True, text=True, check=False)
        self.assertNotIn(str(self.temporary), result.stdout + result.stderr, "a private path was printed")
        self.assertNotIn("bridgevm-a9-import-lane", result.stdout + result.stderr, "a lane slug was printed")
        return result

    def assert_refused(self, result: subprocess.CompletedProcess, names: list[str], text: str = "") -> None:
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertIn("T19 lane vTPM key forget refused: ", result.stderr)
        self.assertIn(text, result.stderr)
        self.assertEqual([call["--stable-vm-id"] for call in self.calls()], names)

    def assert_sealed_inputs(self, requests: list[dict]) -> None:
        """Every call used its lane's sealed source state, package and code, never anything of the booted lane."""
        lanes = {request["lane_root"]: request for request in requests}
        for call in self.calls():
            request = lanes[str(Path(call["--state-dir"]).parents[1])]
            self.assertEqual((call["--state-dir"], call["--package"], call["--recovery-code-file"]),
                             (request["source_vtpm_path"], request["source_vtpm_package_path"], request["source_vtpm_code_path"]))
            bundle = str(Path(request["library_root_path"]) / request["vm_slug"])
            self.assertFalse(any(value.startswith(bundle) for value in call.values()), "the booted lane was used")

    def test_key_under_the_requested_slug_without_a_library_is_forgotten(self) -> None:
        request = self.lane(1, booted=False)
        self.imported(request["vm_slug"])
        result = self.forget()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual([call["--stable-vm-id"] for call in self.calls()], [request["vm_slug"]])
        self.assertFalse(self.held(request["vm_slug"]), "the imported key survived cleanup")
        self.assertIn("1 forgotten, 0 absent", result.stdout)
        self.assert_sealed_inputs([request])

    def test_keys_under_unpublished_directories_are_forgotten_but_not_the_source_cache(self) -> None:
        request = self.lane(1)
        library = Path(request["library_root_path"])
        (library / "windows-11").mkdir()
        (library / "Derived/WindowsInstallSources").mkdir(parents=True)
        (library / "library-index.json").write_text("{}\n")
        self.imported(request["vm_slug"], "windows-11", "Derived", "windows-12")
        result = self.forget()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual([call["--stable-vm-id"] for call in self.calls()], [request["vm_slug"], "windows-11"])
        self.assertFalse(self.held(request["vm_slug"]) or self.held("windows-11"), "an unpublished import's key survived")
        self.assertTrue(self.held("Derived") and self.held("windows-12") and self.held(SOURCE_VM), "another key was touched")
        (library / "Derived/vm.json").write_text("{}\n")
        self.assertEqual(self.forget().returncode, 0)
        self.assertEqual(self.calls()[-3:], [dict(self.calls()[0], **{"--stable-vm-id": name})
                                             for name in ("Derived", request["vm_slug"], "windows-11")])
        self.assertFalse(self.held("Derived"), "a registered Derived VM is a candidate")

    def test_absent_keys_are_shown_by_the_products_missing_key_line(self) -> None:
        request = self.lane(1)
        (Path(request["library_root_path"]) / "windows-11").mkdir()
        result = self.forget()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(len(self.calls()), 2)
        self.assertIn("0 forgotten, 2 absent", result.stdout)

    def test_foreign_stderr_on_exit_1_is_refused(self) -> None:
        request = self.lane(1)
        slug = request["vm_slug"]
        self.imported(slug)
        for marker in ("refuse", "unreadable"):
            with self.subTest(marker):
                self.imported(slug, marker=marker)
                self.assert_refused(self.forget(), [slug] * (1 + (marker == "unreadable")), "without the product's proof")
                (self.keychain / f"{slug}.{marker}").unlink()
        self.assertTrue(self.held(slug), "a refused key disappeared")
        bodies = {"missing-key line after a proof": "print('forgot_imported_key=' + slug); print(missing, file=sys.stderr); sys.exit(1)",
                  "missing-key text inside a longer line": "print(missing + ' (retry)', file=sys.stderr); sys.exit(1)",
                  "missing-key line with exit 2": "print(missing, file=sys.stderr); sys.exit(2)",
                  "killed by a signal": "import os, signal; print(missing, file=sys.stderr); os.kill(os.getpid(), signal.SIGKILL)"}
        for name, body in bodies.items():
            with self.subTest(name):
                self.clear_calls()
                self.script_app(body)
                self.assert_refused(self.forget(), [slug])

    def test_exit_0_without_the_exact_proof_is_refused(self) -> None:
        request = self.lane(1)
        slug = request["vm_slug"]
        self.imported(slug)
        self.imported(slug, marker="silent")
        self.assert_refused(self.forget(), [slug], "forget-import exited 0 without")
        self.assertTrue(self.held(slug))
        bodies = {"proof for another slug": "print('forgot_imported_key=' + slug + '0')",
                  "two proof lines": "print('forgot_imported_key=' + slug); print('forgot_imported_key=' + slug)",
                  "proof with trailing text": "print('forgot_imported_key=' + slug + ' ')",
                  "proof only on stderr": "print('forgot_imported_key=' + slug, file=sys.stderr)"}
        for name, body in bodies.items():
            with self.subTest(name):
                self.clear_calls()
                self.script_app(body)
                self.assert_refused(self.forget(), [slug])
        self.script_app("print('note: Keychain item deleted'); print('forgot_imported_key=' + slug)")
        self.assertEqual(self.forget().returncode, 0, "one exact proof line among other output is the product's proof")

    def test_a_hung_call_is_refused_within_its_bound_and_no_further_call_is_made(self) -> None:
        self.seal("release")
        requests = [self.lane(number) for number in (1, 2, 3)]
        self.imported(*(request["vm_slug"] for request in requests))
        self.imported(requests[0]["vm_slug"], marker="hang")
        tool = load("t19_lane_key_forget", TOOL)
        output = io.StringIO()
        started = time.monotonic()
        with mock.patch.object(tool, "TIMEOUT_SECONDS", 1), contextlib.redirect_stderr(output), contextlib.redirect_stdout(output):
            status = tool.main(["--work", str(self.work), "--verified", str(self.verified)])
        self.assertEqual(status, 1)
        self.assertLess(time.monotonic() - started, 15, "the refusal was not bounded by the call timeout")
        self.assertIn("did not finish within 1 s; 2 further forget-import call(s) not made", output.getvalue())
        self.assertEqual([call["--stable-vm-id"] for call in self.calls()], [requests[0]["vm_slug"]])
        self.assertTrue(all(self.held(request["vm_slug"]) for request in requests[1:]))

    def test_a_changed_executable_runs_nothing(self) -> None:
        request = self.lane(1)
        (Path(request["library_root_path"]) / "windows-11").mkdir()
        self.imported(request["vm_slug"], "windows-11")
        verified = json.loads(self.verified.read_text())
        verified["assets"]["app_executable"]["sha256"] = "0" * 64
        self.verified.write_text(json.dumps(verified))
        self.assert_refused(self.forget(), [], "differs from its sealed SHA-256; nothing was run")
        self.seal()
        with self.executable.open("a") as executable:
            executable.write("# changed after sealing\n")
        self.assert_refused(self.forget(), [], "differs from its sealed SHA-256; nothing was run")
        self.script_app("open(sys.argv[0], 'a').write('# changed by its own call\\n'); print('forgot_imported_key=' + slug)")
        self.assert_refused(self.forget(), [request["vm_slug"]], "library directory: app executable differs from its sealed SHA-256")
        self.assertTrue(self.held("windows-11"))

    def test_the_booted_state_is_never_used(self) -> None:
        request = self.lane(1)
        self.imported(request["vm_slug"])
        result = self.forget()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assert_sealed_inputs([request])
        self.assertFalse(self.held(request["vm_slug"]))
        inputs = Path(request["lane_root"]) / "inputs"
        other = self.temporary / "other"
        (other / "vtpm").mkdir(parents=True)
        (other / "vtpm/tpm2-00.permall").write_bytes(b"another VM's state")
        export_source(self.app, other)
        for name, change in {"sealed state": lambda: (inputs / "vtpm/tpm2-00.permall").write_bytes(b"rewritten"),
                             "package of another VM": lambda: shutil.copy2(other / "vtpm-recovery.json", inputs / "vtpm-recovery.json"),
                             "state replaced by a link": lambda: (shutil.rmtree(inputs / "vtpm"), (inputs / "vtpm").symlink_to(Path(request["vtpm_state_path"])))}.items():
            with self.subTest(name):
                subprocess.run(["chmod", "-R", "u+w", str(inputs)], check=True)
                (inputs / "vtpm").unlink() if (inputs / "vtpm").is_symlink() else shutil.rmtree(inputs / "vtpm")
                shutil.copytree(self.source / "vtpm", inputs / "vtpm")
                shutil.copy2(self.source / "vtpm-recovery.json", inputs / "vtpm-recovery.json")
                change()
                self.imported(request["vm_slug"])
                self.clear_calls()
                self.assert_refused(self.forget(), [], "lane 1 vTPM")
                self.assertTrue(self.held(request["vm_slug"]))

    def test_product_lines_and_candidate_rules_match_the_swift_sources(self) -> None:
        tool, fake = load("t19_lane_key_forget", TOOL), load("fake_vtpm_lifecycle_app", FAKE_APP)
        self.assertEqual(tool.MISSING_KEY, "ERROR: " + MISSING)
        self.assertEqual(fake.MISSING_KEY, MISSING)
        command = (SWIFT / "BridgeVMControl/HvfEngine/VTPMImportCleanupCommand.swift").read_text()
        self.assertIn(f'print("{tool.FORGOT}\\(stableID)")', command)
        self.assertIn('Data("ERROR: \\(error.localizedDescription)\\n".utf8)', command)
        call = (SWIFT / "BridgeVMProductE2E/A9KeyForgetCall.swift").read_text()
        pieces = re.search(r'static let missingKeyLine = ((?:"[^"]*"\s*\+?\s*)+)', call).group(1)
        self.assertEqual("".join(re.findall(r'"([^"]*)"', pieces)), tool.MISSING_KEY)
        self.assertIn(f"static let outputLimit = {tool.OUTPUT_LIMIT:_}", call)
        self.assertIn(f'$0.hasPrefix("{tool.FORGOT}")', call)
        registrations = (SWIFT / "BridgeVMProductE2E/A9LaneRegistrations.swift").read_text()
        self.assertIn(f"static let limit = {tool.LIBRARY_LIMIT}", registrations)
        candidates = (SWIFT / "BridgeVMProductE2E/A9KeyCleanupCandidates.swift").read_text()
        self.assertIn('!(entry == "Derived" && unregistered(directory))', candidates)
        cleanup = (SWIFT / "BridgeVMProductE2E/A9ImportedKeyCleanup.swift").read_text()
        self.assertIn('"--state-dir", request.sourceVtpmPath', cleanup)

    def test_forged_requests_run_nothing_for_their_lane(self) -> None:
        request = self.lane(1)
        self.imported(request["vm_slug"])
        path = Path(request["lane_root"]) / "request.json"
        original, other = path.read_text(), "0" * 12
        forgeries = {
            "slug for another nonce": {"vm_slug": f"bridgevm-a9-import-lane-1-{other}"},
            "product default slug": {"vm_slug": "windows-11"},
            "name for another nonce": {"vm_name": f"BridgeVM A9 Import Lane 1 {other}"},
            "ordinal of another lane": {"lane": 2},
            "booted state as the source": {"source_vtpm_path": request["vtpm_state_path"]},
            "package elsewhere": {"source_vtpm_package_path": str(self.source / "vtpm-recovery.json")},
            "code elsewhere": {"source_vtpm_code_path": "/private/tmp/unrequested-code.txt"},
            "state of another slug": {"vtpm_state_path": request["vtpm_state_path"].replace(request["vm_slug"], "windows-11")},
            "library elsewhere": {"library_root_path": str(self.temporary / "library")},
            "another app": {"app_executable_path": "/Applications/BridgeVM.app/Contents/MacOS/BridgeVMControl"},
            "another runner": {"runner_path": str(self.temporary / "hvf-runner")},
            "T17 schema": {"schema_version": "bridgevm.windows-hvf-3d-off-product-e2e-request.v2"},
            "release mode": {"campaign_mode": "release"},
            "another job": {"job_id": "t19-another-job"},
            "another work identity": {"work_identity": "1:2"},
            "3D injection": {"three_d_injection": True},
            "unknown field": {"extra": True},
        }
        for name, change in forgeries.items():
            with self.subTest(name):
                path.write_text(json.dumps({**json.loads(original), **change}))
                self.assert_refused(self.forget(), [], "lane 1 ")
        path.unlink()
        path.symlink_to(self.temporary / "elsewhere.json")
        self.assert_refused(self.forget(), [], "lane 1 request is not one bounded regular JSON file")
        self.assertTrue(self.held(request["vm_slug"]))

    def test_every_lane_is_attempted_and_every_refusal_reported(self) -> None:
        self.seal("release")
        forged, valid, refused = (self.lane(number) for number in (1, 2, 3))
        self.imported(forged["vm_slug"], valid["vm_slug"], refused["vm_slug"])
        self.imported(refused["vm_slug"], marker="refuse")
        path = Path(forged["lane_root"]) / "request.json"
        path.write_text(json.dumps({**json.loads(path.read_text()), "lane": 3}))
        result = self.forget()
        self.assert_refused(result, [valid["vm_slug"], refused["vm_slug"]])
        self.assertEqual([line.split(": ", 1)[1].split(" ")[:2] for line in result.stderr.splitlines()], [["lane", "1"], ["lane", "3"]])
        self.assertFalse(self.held(valid["vm_slug"]))
        self.assertTrue(self.held(forged["vm_slug"]) and self.held(refused["vm_slug"]))
        self.assertIn("1 forgotten, 0 absent, 2 refusal(s)", result.stdout)

    def test_the_work_tree_holds_only_the_lanes_of_its_campaign(self) -> None:
        empty = self.work / "lane-1"
        empty.mkdir(mode=0o700)
        self.assertEqual(self.forget().returncode, 0, "a lane whose request was never written owns no key")
        (empty / "inputs").mkdir()
        self.assert_refused(self.forget(), [], "lane 1 holds files but no request")
        shutil.rmtree(empty)
        request = self.lane(1)
        for name, make in {"a second pilot lane": lambda path: path.mkdir(mode=0o700), "a stray file": lambda path: path.write_text("")}.items():
            with self.subTest(name):
                stray = self.work / ("lane-2" if "lane" in name else "notes.txt")
                make(stray)
                self.imported(request["vm_slug"])
                self.assert_refused(self.forget(), [request["vm_slug"]] * (1 + ("file" in name)), "not a pilot lane")
                self.assertFalse(self.held(request["vm_slug"]), "a valid lane was not attempted")
                stray.rmdir() if stray.is_dir() else stray.unlink()
        subprocess.run(["chmod", "-R", "u+w", str(self.work)], check=True)
        shutil.rmtree(self.work / "lane-1")
        self.seal("release")
        request = self.lane(1)
        self.imported(request["vm_slug"])
        (self.work / "lane-3").mkdir(mode=0o700)
        self.clear_calls()
        self.assert_refused(self.forget(), [request["vm_slug"]], "lanes do not run consecutively from lane 1")
        self.assertFalse(self.held(request["vm_slug"]), "lane 1 was not attempted")

    def test_the_work_tree_identity_is_checked_before_anything_runs(self) -> None:
        request = self.lane(1)
        self.imported(request["vm_slug"])
        tree, renamed = self.work, self.work.with_name(f"bridgevm-e2e-{JOB}.abcdef")
        tree.rename(renamed)
        self.work = renamed
        self.assert_refused(self.forget(), [], "work tree is not a T19 job tree; nothing was run")
        renamed.rename(tree)
        self.work = tree
        tree.chmod(0o755)
        self.assert_refused(self.forget(), [], "work directory is not private and owned; nothing was run")
        tree.chmod(0o700)
        self.work = self.temporary / "link"
        self.work.symlink_to(tree)
        self.assert_refused(self.forget(), [], "work directory is not canonical; nothing was run")
        self.assertTrue(self.held(request["vm_slug"]))

    def test_unexpected_library_entries_are_refused_but_every_candidate_is_attempted(self) -> None:
        request = self.lane(1)
        library, slug = Path(request["library_root_path"]), request["vm_slug"]
        (library / "windows-11").mkdir()
        (library / "linked-vm").symlink_to(library / "windows-11")
        (library / "bell\x07vm").mkdir()
        self.imported(slug, "windows-11", "linked-vm")
        self.assert_refused(self.forget(), [slug, "windows-11"], "lane 1 library holds a link, special file or unsafe name")
        self.assertEqual(self.forget().stderr.count("refused"), 2)
        self.assertTrue(self.held("linked-vm"))
        for index in range(15):
            (library / f"vm-{index}").mkdir()
        self.clear_calls()
        self.assert_refused(self.forget(), [slug], "lane 1 library holds more entries than one lane creates")
        library.chmod(0)
        self.assert_refused(self.forget(), [slug, slug], "lane 1 library cannot be listed (PermissionError EACCES)")
        library.chmod(0o700)

    def test_output_beyond_its_bound_is_refused(self) -> None:
        request = self.lane(1)
        self.script_app("print('x' * 70_000); print('forgot_imported_key=' + slug)")
        self.assert_refused(self.forget(), [request["vm_slug"]], "forget-import output exceeds 65536 bytes")


if __name__ == "__main__":
    unittest.main()
