"""Owned installer fixtures: no host installer or service command is run."""
from pathlib import Path
import os
import subprocess
import tempfile


class InstallerFixture:
    def __init__(self, source, *, location="checkout", listener=False, free_gib=200):
        self.work = tempfile.TemporaryDirectory(prefix="bridgevm-installer-contract-")
        self.root = Path(self.work.name).resolve()
        self.home = self.root / "home"
        self.queue = self.root / "queue"
        self.home.mkdir()
        (self.home / "Library/LaunchAgents").mkdir(parents=True)
        self.repo = self.home / location if location != "checkout" else self.root / location
        self.scripts = self.repo / "scripts/live-gates"
        self.scripts.mkdir(parents=True)
        self.installer = self.scripts / "install-studio-queue.sh"
        self.installer.write_bytes(source.read_bytes())
        self.worker = self.scripts / "bridgevm-live-worker.sh"
        self.worker.write_text("#!/bin/sh\nexit 98\n")
        self.worker.chmod(0o700)
        self.template = self.scripts / "com.ketchio.bridgevm-live.plist"
        self.template.write_text("owned fixture template\n")
        self.bin = self.root / "bin"
        self.bin.mkdir()
        self.calls = self.root / "calls"
        for name in ("dirname", "awk"):
            (self.bin / name).symlink_to(Path("/usr/bin") / name)
        for name in ("git", "python3", "caffeinate", "taskpolicy", "cargo"):
            self.stub(name, "exit 0\n")
        self.stub("df", f"printf 'Filesystem Blocks Used Available\\nfake 1000 1 {free_gib}\\n'\n")
        self.stub("pgrep", f'printf "pgrep:%s\\n" "$*" >> "$FIXTURE_CALLS"\nexit {0 if listener else 1}\n')
        for name in ("mkdir", "chmod", "sed", "plutil", "launchctl", "id"):
            self.stub(name, f'printf "forbidden:{name}:%s\\n" "$*" >> "$FIXTURE_CALLS"\nexit 97\n')
        self.env = {"HOME": str(self.home), "PATH": str(self.bin), "LC_ALL": "C",
                    "BRIDGEVM_LIVE_ROOT": str(self.queue),
                    "BRIDGEVM_LIVE_MIN_FREE_GIB": "100", "FIXTURE_CALLS": str(self.calls)}

    def stub(self, name, body):
        path = self.bin / name
        path.write_text("#!/bin/sh\n" + body)
        path.chmod(0o700)
        return path

    def inventory(self):
        result = {}
        for base in (self.home, self.queue, self.repo):
            if not base.exists():
                result[str(base)] = None
                continue
            for path in (base, *sorted(base.rglob("*"))):
                state = (path.lstat().st_mode,)
                if path.is_symlink():
                    state += (os.readlink(path),)
                elif path.is_file():
                    state += (path.read_bytes(),)
                result[str(path)] = state
        return result

    def run(self, installer=None):
        before = self.inventory()
        process = subprocess.run(["/bin/bash", str(installer or self.installer), "--dry-run"],
                                 env=self.env, cwd=self.root, text=True,
                                 capture_output=True, timeout=5)
        return process, before, self.inventory(), self.calls.read_text() if self.calls.exists() else ""

    def close(self):
        self.work.cleanup()


def assert_dry_run(test, result):
    process, before, after, calls = result
    test.assertEqual(process.returncode, 0, process.stderr)
    test.assertIn("== dry run, nothing installed ==", process.stdout.splitlines())
    test.assertNotIn("== install ==", process.stdout.splitlines())
    test.assertEqual(before, after, "dry run changed owned home, queue or source")
    test.assertNotIn("forbidden:", calls, "dry run attempted an installation command")


def assert_refusal(test, result, reason):
    process, before, after, calls = result
    test.assertEqual(process.returncode, 1, process.stdout + process.stderr)
    test.assertIn(reason, process.stderr)
    test.assertNotIn("== dry run, nothing installed ==", process.stdout)
    test.assertEqual(before, after, "refusal changed owned home, queue or source")
    test.assertNotIn("forbidden:", calls)
