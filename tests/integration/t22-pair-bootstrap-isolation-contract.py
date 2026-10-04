"""Owned cached stdlib collisions must not load before D10 source admission."""
from pathlib import Path
import py_compile
import shutil
import subprocess
import unittest
from t22_pair_worker_fixtures import WorkerFixture, LIVE

class BootstrapIsolation(unittest.TestCase):
    def setUp(self):
        self.fixture = WorkerFixture(self)
        self.cli = self.fixture.repo / "scripts/live-gates/receipt-cli"
        shutil.copy2(LIVE / "bridgevm-live", self.cli)
        self.fixture.git("add", "scripts/live-gates/receipt-cli")
        self.fixture.git("-c", "user.name=Owned Fixture", "-c", "user.email=fixture@example.invalid",
                         "commit", "-qm", "owned archive bootstrap")
        self.fixture.commit = self.fixture.git("rev-parse", "HEAD").stdout.strip()
        directory = self.fixture.sealed_job(state="running")
        (directory / "receipt.public.json").write_text('{"unverified":true}\n')

    def refused_before_import(self, module):
        source = self.fixture.root / "owned-stdlib-collision.py"
        source.write_text('raise RuntimeError("owned cached stdlib collision")\n')
        cached = self.fixture.repo / "scripts/live-gates" / (module + ".pyc")
        py_compile.compile(str(source), cfile=str(cached), doraise=True)
        self.assertEqual(self.fixture.git("status", "--porcelain", "--untracked-files=all").stdout, "")
        result = subprocess.run([str(self.cli), "receipt", "a-fixture"], env=self.fixture.env(),
                                capture_output=True, text=True, timeout=35)
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(result.stdout, "")
        self.assertNotIn("owned cached stdlib collision", result.stderr)
        self.assertIn("D10 cached repository code refused", result.stderr)

    def test_launcher_stdlib_collision_is_not_imported(self):
        self.refused_before_import("pathlib")

    def test_router_stdlib_collision_is_not_imported(self):
        self.refused_before_import("json")

if __name__ == "__main__": unittest.main()
