#!/usr/bin/env python3
"""Exercise the physical queue worker with synthetic T17 cancellation only."""
import importlib.util
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import sys
import tempfile
import time
import unittest

ROOT = Path(__file__).resolve().parents[2]
LIVE = ROOT / "scripts/live-gates"
SPEC = importlib.util.spec_from_file_location("t17_receipt", ROOT / "scripts/verify-windows-product-e2e-receipt.py")
VERIFY = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(VERIFY)


def receipt(commit, job_id, mode):
    value = VERIFY._fixture("pilot")
    value.update(job_id=job_id, commit=commit, tested_commit=commit,
                 hosted_ci_commit=commit, run_count=1, passes=0, failures=1,
                 elapsed_ms=0, started_at="2026-09-01T00:00:00Z",
                 finished_at="2026-09-01T00:00:00Z", clean_machine=False,
                 ui_frontend_automated=False, product_model_automated=False,
                 worker_cleanup_verified=(mode == "clean"),
                 claim_eligible=False, hosted_ci_green=False,
                 security_ci_green=False, artifact_signing_class="development-signed",
                 outcome="canceled" if mode == "clean" else "cleanup-failed",
                 failure_code="canceled" if mode == "clean" else "cleanup-failed")
    value["pass"] = False
    value.update({field: "absent" for field in VERIFY.HASH_FIELDS})
    value.update({field: 0 for field in VERIFY.STAGE_FIELDS})
    VERIFY.validate(value, expected_commit=commit)
    return value


FAKE_CLI = """#!/bin/bash
set -euo pipefail
[[ "$1" == next ]] || exit 2
for source in "$BRIDGEVM_LIVE_ROOT"/queued/*; do
  [[ -d "$source" ]] || exit 1
  target="$BRIDGEVM_LIVE_ROOT/running/$(basename "$source")"
  mv "$source" "$target"
  printf '%s\\n' "$target"
  exit 0
done
exit 1
"""

FAKE_TIER = """#!/bin/bash
set -euo pipefail
shift
out= job=
while [[ $# -gt 0 ]]; do
  case "$1" in
    --out) out="$2"; shift 2 ;;
    --job-id) job="$2"; shift 2 ;;
    *) shift ;;
  esac
done
repo="$(cd "$(dirname "$0")/../.." && pwd)"
mode="$(cat "$out/mode")"
if [[ "$mode" == survivor ]]; then
  python3 - "$out" <<'PY'
import os, pathlib, subprocess, sys
child = subprocess.Popen([sys.executable, '-c', 'import time; time.sleep(30)',
                          sys.argv[1]], start_new_session=True,
                         stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL,
                         stderr=subprocess.DEVNULL)
pathlib.Path(sys.argv[1], 'survivor.pid').write_text(str(child.pid))
PY
fi
finish() {
  python3 "$repo/scripts/live-gates/fixture-receipt.py" "$out" "$job" "$mode" "$repo"
  exit 130
}
trap finish TERM
: > "$out/ready"
while :; do sleep 1 || true; done
"""

FAKE_RECEIPT = """#!/usr/bin/env python3
import importlib.util, json, pathlib, subprocess, sys
out, job, mode, repo = sys.argv[1:]
if mode == 'missing': raise SystemExit(0)
target = pathlib.Path(out) / 'receipt.json'
if mode == 'invalid': target.write_text('{"unverified":true}\\n'); raise SystemExit(0)
source = pathlib.Path(repo) / 'scripts/verify-windows-product-e2e-receipt.py'
spec = importlib.util.spec_from_file_location('verify', source)
verify = importlib.util.module_from_spec(spec)
spec.loader.exec_module(verify)
commit = subprocess.check_output(['git', '-C', repo, 'rev-parse', 'HEAD'], text=True).strip()
value = verify._fixture('pilot')
value.update(job_id=job, commit=commit, tested_commit=commit, hosted_ci_commit=commit,
             run_count=1, passes=0, failures=1, elapsed_ms=0,
             started_at='2026-09-01T00:00:00Z', finished_at='2026-09-01T00:00:00Z',
             clean_machine=False, ui_frontend_automated=False,
             product_model_automated=False, worker_cleanup_verified=(mode == 'clean'),
             claim_eligible=False, hosted_ci_green=False, security_ci_green=False,
             artifact_signing_class='development-signed',
             outcome='canceled' if mode == 'clean' else 'cleanup-failed',
             failure_code='canceled' if mode == 'clean' else 'cleanup-failed')
value['pass'] = False
value.update({key: 'absent' for key in verify.HASH_FIELDS})
value.update({key: 0 for key in verify.STAGE_FIELDS})
verify.validate(value, expected_commit=commit)
target.write_text(json.dumps(value) + '\\n')
"""


class T17WorkerCleanupFence(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="t17-worker-fence-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.repo = self.root / "repo"
        scripts = self.repo / "scripts/live-gates"
        scripts.mkdir(parents=True)
        for name in ("bridgevm-live-worker.sh", "live-process-cleanup.sh",
                     "app-ui-host-worker-cleanup.sh", "publish-receipt.sh",
                     "redact-receipt.py", "recover-stale-jobs.sh",
                     "recover-stale-receipt.sh"):
            shutil.copy2(LIVE / name, scripts / name)
        shutil.copy2(ROOT / "scripts/verify-windows-product-e2e-receipt.py",
                     self.repo / "scripts/verify-windows-product-e2e-receipt.py")
        for name in ("t17-worker-cleanup-fence.sh",):
            source = LIVE / name
            if source.exists():
                shutil.copy2(source, scripts / name)
        for name, body in (("bridgevm-live", FAKE_CLI), ("run-tier.sh", FAKE_TIER),
                           ("fixture-receipt.py", FAKE_RECEIPT),
                           ("write-missing-receipt.sh", "#!/bin/bash\nexit 0\n")):
            target = scripts / name
            target.write_text(body)
            target.chmod(0o755)
        subprocess.run(["git", "-C", str(self.repo), "init", "-q"], check=True)
        subprocess.run(["git", "-C", str(self.repo), "add", "-A"], check=True)
        subprocess.run(["git", "-C", str(self.repo), "-c", "user.name=Fixture",
                        "-c", "user.email=fixture@example.invalid", "commit", "-qm", "fixture"], check=True)
        self.commit = subprocess.check_output(["git", "-C", str(self.repo), "rev-parse", "HEAD"], text=True).strip()
        self.queue = self.root / "queue"
        self.work = self.root / "work"
        for state in ("queued", "running", "done"):
            (self.queue / state).mkdir(parents=True)

    def job(self, state, name="a-fixture", mode="survivor", tier="t17-windows-hvf-product-e2e"):
        directory = self.queue / state / name
        directory.mkdir()
        (directory / "job.env").write_text(f"job_id={name}\ntier={tier}\ncommit={self.commit}\n")
        (directory / "mode").write_text(mode)
        return directory

    def env(self):
        return dict(os.environ, BRIDGEVM_REPO=str(self.repo), BRIDGEVM_LIVE_ROOT=str(self.queue),
                    BRIDGEVM_LIVE_WORK=str(self.work), BRIDGEVM_LIVE_MIN_FREE_GIB="0")

    def cancel_tier(self, mode):
        self.job("queued", mode=mode)
        worker = subprocess.Popen([str(self.repo / "scripts/live-gates/bridgevm-live-worker.sh")],
                                  env=self.env(), stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        self.addCleanup(self.stop_owned_worker, worker)
        deadline = time.monotonic() + 15
        while not (self.queue / "running/a-fixture/ready").exists() and time.monotonic() < deadline:
            self.assertIsNone(worker.poll(), "worker exited before synthetic tier was ready")
            time.sleep(0.05)
        running = self.queue / "running/a-fixture"
        self.assertTrue((running / "ready").exists())
        (running / "cancel.requested").touch()
        output, error = worker.communicate(timeout=25)
        return worker.returncode, (output + error).decode(errors="replace")

    def test_canceled_survivor_fences_worker_and_preserves_worktree(self):
        self.job("queued", "z-next", "clean", "t0-check")
        status, output = self.cancel_tier("survivor")
        running = self.queue / "running/a-fixture"
        pid_file = running / "survivor.pid"
        if not pid_file.exists():
            pid_file = self.queue / "done/a-fixture/survivor.pid"
        pid = int(pid_file.read_text()) if pid_file.exists() else 0
        if pid:
            self.addCleanup(self.stop_owned_survivor, pid, str(running))
        self.assertEqual(status, 126, output)
        self.assertTrue((self.queue / "worker-cleanup-required").is_file())
        self.assertTrue(running.is_dir())
        self.assertTrue((self.work / "a-fixture").is_dir())
        self.assertTrue((self.queue / "queued/z-next").is_dir())
        self.assertEqual(json.loads((running / "receipt.json").read_text())["worker_cleanup_verified"], False)
        self.assertGreater(pid, 1)
        self.assertTrue(subprocess.run(["ps", "-o", "command=", "-p", str(pid)],
                                       capture_output=True, text=True).stdout.strip().endswith(str(running)))

    def test_missing_receipt_fences_worker(self):
        status, output = self.cancel_tier("missing")
        self.assertEqual(status, 126, output)
        self.assertTrue((self.queue / "worker-cleanup-required").is_file())
        self.assertTrue((self.queue / "running/a-fixture").is_dir())
        self.assertTrue((self.work / "a-fixture").is_dir())

    def test_invalid_receipt_fences_worker(self):
        status, output = self.cancel_tier("invalid")
        self.assertEqual(status, 126, output)
        self.assertTrue((self.queue / "worker-cleanup-required").is_file())
        self.assertTrue((self.queue / "running/a-fixture").is_dir())
        self.assertTrue((self.work / "a-fixture").is_dir())

    def test_verified_cleanup_allows_worker_finalization(self):
        status, output = self.cancel_tier("clean")
        self.assertEqual(status, 0, output)
        self.assertFalse((self.queue / "worker-cleanup-required").exists())
        self.assertTrue((self.queue / "done/a-fixture").is_dir())
        self.assertFalse((self.work / "a-fixture").exists())

    @staticmethod
    def stop_owned_worker(worker):
        if worker.poll() is None:
            worker.terminate()
            worker.communicate(timeout=5)

    @staticmethod
    def stop_owned_survivor(pid, marker):
        command = subprocess.run(["ps", "-o", "command=", "-p", str(pid)],
                                 capture_output=True, text=True).stdout
        if marker in command:
            os.kill(pid, signal.SIGTERM)

    def test_stale_recovery_requires_strict_cleanup_proof(self):
        for mode in ("survivor", "invalid", "missing", "clean"):
            with self.subTest(mode=mode), tempfile.TemporaryDirectory(dir=self.root) as temp:
                queue = Path(temp) / "queue"
                work = Path(temp) / "work"
                for state in ("running", "done"):
                    (queue / state).mkdir(parents=True)
                job = queue / "running/a-fixture"
                job.mkdir()
                (job / "job.env").write_text(f"job_id=a-fixture\ntier=t17-windows-hvf-product-e2e\ncommit={self.commit}\n")
                work.mkdir()
                subprocess.run(["git", "-C", str(self.repo), "worktree", "add", "--detach",
                                str(work / "a-fixture"), self.commit], capture_output=True, check=True)
                if mode == "invalid":
                    (job / "receipt.json").write_text('{"unverified":true}\n')
                elif mode != "missing":
                    (job / "receipt.json").write_text(json.dumps(receipt(self.commit, "a-fixture", mode)) + "\n")
                result = subprocess.run([str(LIVE / "recover-stale-jobs.sh"), str(self.repo),
                                         str(queue), str(work)], capture_output=True, text=True, timeout=10)
                if mode == "clean":
                    self.assertEqual(result.returncode, 0, result.stderr)
                    self.assertTrue((queue / "done/a-fixture").is_dir())
                    self.assertFalse((work / "a-fixture").exists())
                else:
                    self.assertEqual(result.returncode, 126, result.stderr)
                    self.assertTrue((queue / "worker-cleanup-required").is_file())
                    self.assertTrue(job.is_dir())
                    self.assertTrue((work / "a-fixture").is_dir())


if __name__ == "__main__":
    unittest.main()
