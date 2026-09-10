#!/usr/bin/env python3
"""Refuse deterministic tiers before resolving any sealed revision."""
import os
import pathlib
import shutil
import subprocess
import tempfile
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[2]


class WorkerVenueContracts(unittest.TestCase):
    def exercise(self, tier, sha):
        with tempfile.TemporaryDirectory() as temporary:
            root = pathlib.Path(temporary)
            repo = root / "repo"
            helpers = repo / "scripts/live-gates"
            helpers.mkdir(parents=True)
            queue = root / "queue"
            for state in ("queued", "running", "done"):
                (queue / state).mkdir(parents=True)
            job = queue / "queued/fixture"
            job.mkdir()
            (job / "job.env").write_text("job_id=fixture\ntier=" + tier + "\ncommit=" + sha + "\n")
            shutil.copyfile(ROOT / "scripts/live-gates/bridgevm-live-worker.sh", helpers / "worker.sh")
            scripts = {
                helpers / "live-process-cleanup.sh": "bridgevm_wait_for_tier_group() { return 126; }\n",
                helpers / "recover-stale-jobs.sh": "#!/bin/sh\nexit 0\n",
                helpers / "bridgevm-live": """#!/bin/sh
set -eu
[ "$1" = next ] || exit 2
source="$BRIDGEVM_LIVE_ROOT/queued/fixture"
target="$BRIDGEVM_LIVE_ROOT/running/fixture"
[ -d "$source" ] || exit 1
mv "$source" "$target"
printf '%s\\n' "$target"
""",
            }
            shim = root / "bin"
            shim.mkdir()
            scripts[shim / "git"] = '#!/bin/sh\nprintf called >> "$VENUE_GIT_MARKER"\nexit 77\n'
            scripts[shim / "df"] = '#!/bin/sh\nprintf "Filesystem Size Used Available\\nfixture 999 0 999\\n"\n'
            for path, content in scripts.items():
                path.write_text(content)
                path.chmod(0o700)
            marker = root / "git-called"
            env = dict(os.environ, BRIDGEVM_REPO=str(repo), BRIDGEVM_LIVE_ROOT=str(queue),
                       BRIDGEVM_LIVE_WORK=str(root / "work"), BRIDGEVM_LIVE_MIN_FREE_GIB="0",
                       VENUE_GIT_MARKER=str(marker), PATH=str(shim) + os.pathsep + os.environ["PATH"])
            result = subprocess.run(["bash", str(helpers / "worker.sh")], env=env,
                                    capture_output=True, timeout=10)
            self.assertEqual(result.returncode, 0, result.stderr.decode())
            self.assertFalse((queue / "running/fixture").exists())
            outcome = (queue / "done/fixture/result.env").read_text()
            return outcome, marker.exists()

    def test_t0_is_refused_without_resolving_either_revision(self):
        # These are deliberately unresolved SHA fixtures: policy must run first.
        for sha in ("1" * 40, "2" * 40):
            with self.subTest(sha=sha):
                outcome, git_called = self.exercise("t0-check", sha)
                self.assertEqual(outcome, "result=refused-deterministic-venue\n")
                self.assertFalse(git_called)

    def test_hardware_tier_still_reaches_revision_resolution(self):
        outcome, git_called = self.exercise("t1-vtimer", "3" * 40)
        self.assertEqual(outcome, "result=refused-unknown-commit\n")
        self.assertTrue(git_called)


if __name__ == "__main__":
    unittest.main()
