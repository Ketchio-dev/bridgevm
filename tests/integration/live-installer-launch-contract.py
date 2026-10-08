#!/usr/bin/env python3
"""Render and execute the SSD launch contract using only owned/stubbed assets."""
from pathlib import Path
import plistlib
import shutil
import subprocess
import unittest

from live_installer_fixture import InstallerFixture


REPO = Path(__file__).resolve().parents[2]
SCRIPTS = REPO / "scripts/live-gates"
LABEL = "com.ketchio.bridgevm-live"
PREFIX = ["/bin/bash", "-p", "-c", 'exec "$@"', "bridgevm-live-launch"]


class LaunchContract(unittest.TestCase):
    def fixture(self, custom=False):
        fixture = InstallerFixture(SCRIPTS / "install-studio-queue.sh", location="source checkout")
        self.addCleanup(fixture.close)
        external = fixture.root / "external volume"
        external.mkdir()
        if not custom: (fixture.home / "BridgeVM").symlink_to(external, target_is_directory=True)
        fixture.queue = external / "queue & custom" if custom else fixture.home / "BridgeVM/live-queue"
        fixture.env.update(BRIDGEVM_LIVE_ROOT=str(fixture.queue)) if custom else fixture.env.pop("BRIDGEVM_LIVE_ROOT")
        fixture.template.write_bytes((SCRIPTS / f"{LABEL}.plist").read_bytes())
        for name in ("mkdir", "chmod", "sed", "id", "bash"):
            path = fixture.bin / name
            path.unlink(missing_ok=True)
            path.symlink_to(shutil.which(name))
        fixture.stub("plutil", "exit 0\n")  # plistlib below authenticates rendered syntax.
        fixture.stub("launchctl", 'printf "launchctl:%s\\n" "$*" >> "$FIXTURE_CALLS"\n')
        result = subprocess.run(["/bin/bash", "-c", 'umask 002; exec /bin/bash "$1"', "fixture", str(fixture.installer)], env=fixture.env,
                                cwd=fixture.root, text=True, capture_output=True, timeout=10)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        plist = fixture.home / "Library/LaunchAgents" / f"{LABEL}.plist"
        return fixture, plistlib.loads(plist.read_bytes())

    def test_rendered_launch_preserves_sanitized_worker_environment_and_schedule(self):
        fixture, plist = self.fixture()
        home = str(fixture.home)
        user = subprocess.check_output(["id", "-un"], text=True).strip()
        path = f"{home}/.cargo/bin:/opt/homebrew/opt/rustup/bin:/opt/homebrew/bin:/usr/bin:/bin:/usr/sbin:/sbin"
        self.assertEqual(plist["ProgramArguments"], PREFIX + [
            "/usr/bin/env", "-i", f"HOME={home}", f"USER={user}", f"LOGNAME={user}",
            "SHELL=/bin/bash", f"PATH={path}", f"BRIDGEVM_LIVE_ROOT={fixture.queue.resolve()}",
            f"BRIDGEVM_LIVE_WORK={(fixture.home / 'BridgeVM/live-work').resolve()}", "BRIDGEVM_LIVE_MIN_FREE_GIB=100", str(fixture.worker)])
        self.assertEqual(plist["Label"], LABEL)
        self.assertEqual(plist["StartInterval"], 60)
        self.assertIs(plist["RunAtLoad"], False)
        self.assertIs(plist["AbandonProcessGroup"], False)
        logs = fixture.home / "Library/Logs/BridgeVM"
        self.assertEqual(plist["StandardOutPath"], str(logs / "worker.out.log"))
        self.assertEqual(plist["StandardErrorPath"], str(logs / "worker.err.log"))
        self.assertEqual(logs.stat().st_mode & 0o777, 0o700)
        self.assertEqual(fixture.queue.stat().st_mode & 0o777, 0o700)
        for state in ("queued", "running", "done", "job-ledger"): self.assertEqual((fixture.queue / state).stat().st_mode & 0o777, 0o700)
        self.assertFalse((fixture.queue / "logs").exists())
        self.assertIn("external volume", str(fixture.queue.resolve()))
        self.assertNotIn("external volume", str(logs.resolve()))
        calls = fixture.calls.read_text()
        self.assertEqual(calls.count("launchctl:"), 3)
        self.assertIn("launchctl:bootstrap gui/", calls)

    def test_wrapper_preserves_spaced_arguments_and_exit_but_drops_inherited_overrides(self):
        fixture, plist = self.fixture()
        fixture.worker.write_text('''#!/bin/bash
printf '%s\n' "$HOME" "$0" "$BRIDGEVM_LIVE_ROOT"
if [ "${SENTINEL_SECRET+x}${BRIDGEVM_REPO+x}" ]; then exit 99; fi
exit 37
''')
        hook = fixture.root / "inherited-hook.sh"
        hook.write_text('printf hook-ran; exit 96\n')
        env = dict(PATH="/usr/bin:/bin", SENTINEL_SECRET="owned fixture sentinel", BASH_ENV=str(hook),
                   BRIDGEVM_REPO="wrong repo", BRIDGEVM_LIVE_ROOT="wrong queue")
        result = subprocess.run(plist["ProgramArguments"], env=env, text=True,
                                capture_output=True, timeout=10)
        self.assertEqual(result.returncode, 37, result.stderr)
        self.assertEqual(result.stdout.splitlines(), [str(fixture.home), str(fixture.worker), str(fixture.queue.resolve())])
        control = plist["ProgramArguments"].copy()
        control.remove("-p")
        result = subprocess.run(control, env=env, text=True, capture_output=True, timeout=10)
        self.assertEqual(result.returncode, 96, result.stderr)
        self.assertEqual(result.stdout, "hook-ran")

    def test_real_worker_fence_precedes_recovery_and_claim_through_wrapper(self):
        fixture, plist = self.fixture()
        for name in ("bridgevm-live-worker.sh", "live_storage_capacity.py", "live-process-cleanup.sh",
                     "app-ui-host-worker-cleanup.sh", "t17-worker-cleanup-fence.sh"):
            (fixture.scripts / name).write_bytes((SCRIPTS / name).read_bytes())
        for name in ("recover-stale-jobs.sh", "bridgevm-live"):
            path = fixture.scripts / name
            path.write_text('#!/bin/bash\nprintf forbidden > "$HOME/forbidden-call"\nexit 97\n')
            path.chmod(0o700)
        fence = fixture.queue / "worker-cleanup-required"
        fence.write_text("owned fixture fence\n")
        job = fixture.queue / "queued/owned-job"
        job.mkdir()
        (job / "job.env").write_text("job_id=owned-job\n")
        before = fixture.inventory()
        result = subprocess.run(plist["ProgramArguments"], text=True,
                                capture_output=True, timeout=10)
        self.assertEqual(result.returncode, 126, result.stdout + result.stderr)
        self.assertIn("worker is fenced: cleanup needs operator review", result.stdout)
        self.assertEqual(before, fixture.inventory())
        self.assertFalse((fixture.home / "forbidden-call").exists())
        self.assertFalse((fixture.queue / "worker.lock").exists())


if __name__ == "__main__":
    unittest.main()
