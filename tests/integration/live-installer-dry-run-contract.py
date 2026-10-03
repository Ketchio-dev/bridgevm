#!/usr/bin/env python3
"""Exercise the queue installer's dry-run contract without touching host assets."""
from pathlib import Path
import subprocess
import unittest

from live_installer_fixture import InstallerFixture, assert_dry_run, assert_refusal


REPO = Path(__file__).resolve().parents[2]
INSTALLER = REPO / "scripts/live-gates/install-studio-queue.sh"
POLICY = REPO / "tests/integration/live-gate-policy-smoke.sh"


class DryRunContract(unittest.TestCase):
    def fixture(self, **options):
        fixture = InstallerFixture(INSTALLER, **options)
        self.addCleanup(fixture.close)
        return fixture

    def test_supported_dry_run_changes_no_files_and_calls_no_service(self):
        fixture = self.fixture()
        result = fixture.run()
        assert_dry_run(self, result)
        self.assertEqual(result[3], "pgrep:-qf Runner.Listener\n")

    def test_low_space_warning_preserves_the_supported_dry_run(self):
        result = self.fixture(free_gib=20).run()
        assert_dry_run(self, result)
        self.assertIn("warning: 20GiB free, below the 100GiB job guard", result[0].stdout)

    def test_runner_listener_refuses_the_dry_run(self):
        assert_refusal(self, self.fixture(listener=True).run(), "GitHub Actions runner is present")

    def test_runner_directory_refuses_without_probing_processes(self):
        fixture = self.fixture()
        (fixture.home / "actions-runner").mkdir()
        result = fixture.run()
        assert_refusal(self, result, "GitHub Actions runner is present")
        self.assertEqual(result[3], "")

    def test_privacy_protected_source_paths_refuse(self):
        for folder in ("Desktop", "Documents", "Downloads"):
            with self.subTest(folder=folder):
                fixture = self.fixture(location=f"{folder}/checkout")
                assert_refusal(self, fixture.run(), "LaunchAgent privacy policy")

    def test_a_source_symlink_cannot_bypass_the_physical_path_guard(self):
        fixture = self.fixture(location="Documents/checkout")
        alias = fixture.root / "outside-documents"
        alias.symlink_to(fixture.repo, target_is_directory=True)
        assert_refusal(self, fixture.run(alias / "scripts/live-gates/install-studio-queue.sh"),
                       "LaunchAgent privacy policy")

    def test_missing_local_inputs_refuse(self):
        for name, reason in (("worker", "worker is not executable"),
                             ("template", "missing plist template")):
            with self.subTest(input=name):
                fixture = self.fixture()
                getattr(fixture, name).unlink()
                assert_refusal(self, fixture.run(), reason)

    def test_missing_required_tools_refuse(self):
        for name in ("git", "python3", "caffeinate", "taskpolicy", "cargo"):
            with self.subTest(tool=name):
                fixture = self.fixture()
                (fixture.bin / name).unlink()
                assert_refusal(self, fixture.run(), "is required" if name != "caffeinate"
                               and name != "taskpolicy" else "are required")

    def test_contract_detects_a_successful_exit_without_the_dry_run_marker(self):
        fixture = self.fixture()
        text = fixture.installer.read_text().replace('echo "== dry run, nothing installed =="', ":")
        fixture.installer.write_text(text)
        with self.assertRaises(AssertionError):
            assert_dry_run(self, fixture.run())

    def test_contract_detects_an_owned_file_write_despite_success(self):
        fixture = self.fixture()
        text = fixture.installer.read_text().replace('echo "== dry run, nothing installed =="',
                                                    'printf mutation > "$PLIST"\n    echo "== dry run, nothing installed =="')
        fixture.installer.write_text(text)
        with self.assertRaises(AssertionError):
            assert_dry_run(self, fixture.run())

    def test_policy_assertion_refuses_a_failed_contract_command(self):
        source = POLICY.read_text()
        start = source.index("check() {\n")
        function = source[start:source.index("\n}\n", start) + 3]
        assertion = next(line for line in source.splitlines()
                         if line.startswith('check "the installer supports a dry run"'))
        fixture = self.fixture()
        failing = fixture.stub("python3", "exit 42\n")
        command = 'set -euo pipefail\nREPO="$1"\nINSTALL="$2"\nchecks=0\n' + function + "\n" + assertion
        result = subprocess.run(["/bin/bash", "-c", command, "fixture", str(fixture.repo), str(failing)],
                                env=fixture.env, text=True, capture_output=True, timeout=5)
        self.assertEqual(result.returncode, 1, "policy accepted a contract command that exited 42")
        self.assertIn("FAIL: the installer supports a dry run", result.stderr)


if __name__ == "__main__":
    unittest.main()
