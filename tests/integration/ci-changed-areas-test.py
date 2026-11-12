#!/usr/bin/env python3
"""Run ci.yml's changed-areas step against fixed file lists.

clippy, msrv and the venus check are required checks that skip a pull request
when no Rust build input changed. A build input missing from that list would
let a lint or MSRV break through review, so this runs the step itself, taken
from ci.yml, with a stub `git diff`. It also resolves every include_str! and
include_bytes! in the tree and requires each target to count as a build
input, and requires the build scripts to read no files, which the list
assumes. The macOS host contracts also run for the files they read that the
`rust` area ignores; HOST_CONTRACT_INPUTS has one fixture per such file.

    python3 tests/integration/ci-changed-areas-test.py
"""

import os
import pathlib
import re
import subprocess
import sys
import tempfile
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[2]
CI = ROOT / ".github/workflows/ci.yml"

SKIP_BUILD = {
    "Swift-only": ["apps/macos/Sources/BridgeVMControl/AppUIHost.swift",
                   "apps/macos/Tests/AppleVzRunnerTests/AppleVzRunnerCommandTests.swift"],
    "scripts-only": ["scripts/check-project.sh", "scripts/live-gates/guest_input_queue.py",
                     "scripts/win-assets/bv-window-inventory.cs", "tests/integration/guest-input-queue-test.py"],
    "docs-only": ["README.md", "docs/app-cli-create.md", "site/icon.png"],
}
RUN_BUILD = {
    ".rs": ["crates/bridgevm-hvf/src/lib.rs"],
    ".rs outside the crates": ["scripts/live-gates/qmp-close-race-baseline.rs"],
    "Cargo.lock": ["Cargo.lock"],
    "rust-toolchain": ["rust-toolchain"],
    "rust-toolchain.toml": ["rust-toolchain.toml"],
    "ci.yml": [".github/workflows/ci.yml"],
    "build.rs": ["crates/bridgevm-hvf/build.rs"],
    "include_str! BvAgentEvent.swift": ["apps/macos/Sources/BridgeVMControl/HvfEngine/BvAgentEvent.swift"],
    "include_str! HvfClipboardPaste.swift": ["apps/macos/Sources/BridgeVMControl/HvfEngine/HvfClipboardPaste.swift"],
    "include_str! HvfLegacyInputActivity.swift": [
        "apps/macos/Sources/BridgeVMControl/HvfEngine/HvfLegacyInputActivity.swift"],
    "include_str! windows-hvf.json": ["capabilities/windows-hvf.json"],
    "include_str! bvagent.ps1": ["scripts/win-assets/bvagent.ps1"],
    "Swift plus one .rs": ["apps/macos/Sources/BridgeVMControl/AppUIHost.swift", "crates/bridgevm-core/src/lib.rs"],
}
# Files the four macOS host contract suites read that the `rust` area ignores,
# found by running the suites and recording which such files they read.
HOST_CONTRACT_INPUTS = {
    "qmp-stress.yml only (live gate policy)": [".github/workflows/qmp-stress.yml"],
}
INCLUDE_TOKEN = re.compile(r"\binclude(?:_str|_bytes)?!")
INCLUDE = re.compile(r'include(?:_str|_bytes)?!\s*\(\s*(?:concat!\s*\(\s*env!\s*\(\s*"(CARGO_MANIFEST_DIR|OUT_DIR)"'
                     r'\s*\)\s*,\s*)?"([^"]+)"')


def diff_step():
    import yaml
    workflow = yaml.safe_load(CI.read_text())
    job = workflow["jobs"]["changes"]
    step = next(step for step in job["steps"] if step.get("id") == "diff")
    # GitHub runs a step without `shell:` as `bash -e {0}`; so does this test.
    assert "shell" not in step and "defaults" not in workflow and "defaults" not in job, "diff step shell changed"
    return step["run"]


class ChangedAreas(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.work = pathlib.Path(tempfile.mkdtemp())
        (cls.work / "step.sh").write_text(diff_step())
        git = cls.work / "git"
        git.write_text('#!/bin/sh\n[ "$1 $2" = "diff --name-only" ] || exit 64\ncat "$CHANGED_FILES"\n')
        git.chmod(0o755)

    @classmethod
    def tearDownClass(cls):
        for path in cls.work.iterdir():
            path.unlink()
        cls.work.rmdir()

    def areas(self, files, event="pull_request"):
        (self.work / "files").write_text("".join(path + "\n" for path in files))
        output = self.work / "output"
        output.write_text("")
        env = dict(os.environ, PATH=f"{self.work}{os.pathsep}{os.environ['PATH']}", EVENT=event, BASE="0" * 40,
                   GITHUB_OUTPUT=str(output), CHANGED_FILES=str(self.work / "files"))
        subprocess.run(["bash", "-e", str(self.work / "step.sh")], env=env, check=True, stdout=subprocess.DEVNULL)
        return dict(line.split("=", 1) for line in output.read_text().splitlines())

    def test_compile_only_jobs_skip_what_cannot_affect_rust(self):
        for name, files in SKIP_BUILD.items():
            with self.subTest(name):
                areas = self.areas(files)
                self.assertEqual(areas["rust_build"], "false")
                # test (macos-15) and the macOS host contracts still run.
                expected = "false" if name == "docs-only" else "true"
                self.assertEqual((areas["rust"], areas.get("host_contracts")), (expected, expected))

    def test_host_contracts_run_for_the_files_they_read(self):
        for name, files in HOST_CONTRACT_INPUTS.items():
            with self.subTest(name):
                areas = self.areas(files)
                self.assertEqual(areas["rust"], "false")  # the explicit list is what runs them
                self.assertEqual(areas.get("host_contracts"), "true")

    def test_rust_build_inputs_run_them(self):
        for name, files in RUN_BUILD.items():
            with self.subTest(name):
                self.assertEqual(self.areas(files)["rust_build"], "true")

    def test_swift_compile_guard_uses_the_swift_area_on_prs_only(self):
        import yaml
        jobs = yaml.safe_load(CI.read_text())["jobs"]
        guard = jobs["swift-compile-macos-26"]
        self.assertEqual(guard["needs"], "changes")
        self.assertEqual(guard["runs-on"], "macos-26")
        self.assertIn("github.event_name == 'pull_request'", guard["if"])
        self.assertIn("needs.changes.outputs.swift == 'true'", guard["if"])
        self.assertIn("needs.changes.result != 'success'", guard["if"])
        for path in ("apps/macos/Tests/BridgeVMControlTests/HvfFramebufferDamageTests.swift",
                     "scripts/run-xctest-shim-suite.sh", ".github/workflows/ci.yml"):
            with self.subTest(path):
                self.assertEqual(self.areas([path])["swift"], "true")
        self.assertEqual(self.areas(SKIP_BUILD["docs-only"])["swift"], "false")

    def test_main_and_scheduled_runs_run_everything(self):
        for event in ("push", "schedule", "workflow_dispatch"):
            with self.subTest(event):
                self.assertEqual(self.areas(SKIP_BUILD["docs-only"], event),
                                 {"rust": "true", "rust_build": "true", "host_contracts": "true", "swift": "true",
                                  "venus": "true"})

    def test_every_include_target_is_a_rust_build_input(self):
        targets = set()
        sources = subprocess.run(["git", "ls-files", "*.rs"], cwd=ROOT, check=True, capture_output=True,
                                 text=True).stdout.split()
        for source in sources:
            path = ROOT / source
            text = "\n".join("" if line.lstrip().startswith("//") else line
                             for line in path.read_text(errors="replace").splitlines())
            for token in INCLUDE_TOKEN.finditer(text):
                where = f"{source}:{text.count(chr(10), 0, token.start()) + 1}"
                match = INCLUDE.match(text, token.start())
                self.assertIsNotNone(match, f"{where}: unrecognised include form; teach this test to resolve it")
                base, relative = match.groups()
                if base == "OUT_DIR":
                    continue  # generated by a build script, which reads no files
                directory = path.parent
                if base == "CARGO_MANIFEST_DIR":
                    while not (directory / "Cargo.toml").is_file():
                        directory = directory.parent
                    relative = relative.lstrip("/")
                target = pathlib.Path(os.path.normpath(directory / relative))
                self.assertTrue(target.is_file(), f"{where}: include target {target} is missing")
                targets.add(target.relative_to(ROOT).as_posix())
        self.assertIn("scripts/win-assets/bvagent.ps1", targets)
        for target in sorted(targets):
            with self.subTest(target):
                self.assertEqual(self.areas([target])["rust_build"], "true")

    def test_build_scripts_read_no_files(self):
        scripts = subprocess.run(["git", "ls-files", "build.rs", "*/build.rs"], cwd=ROOT, check=True,
                                 capture_output=True, text=True).stdout.split()
        self.assertTrue(scripts)
        for script in scripts:
            with self.subTest(script):
                text = (ROOT / script).read_text()
                self.assertIsNone(re.search(r"\bfs::|\bFile::|include_(?:str|bytes)!|rerun-if-changed=", text),
                                  "a build script reads files now: add them to rust_build in ci.yml")


if __name__ == "__main__":
    try:
        import yaml  # noqa: F401
    except ModuleNotFoundError:
        if os.environ.get("CI"):
            sys.exit("changed areas: FAIL (PyYAML is required here)")
        print("changed areas: SKIP (PyYAML not installed)")
        sys.exit(0)
    unittest.main()
