"""Copied physical worker with exclusively owned, guest-free queue fixtures."""
import hashlib
import os
from pathlib import Path
import shutil
import signal
import subprocess
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parents[2]
LIVE = ROOT / "scripts/live-gates"
TIER = "d10-t22-owned-pair-preparation"

CLAIM = """#!/bin/bash
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

RUN_TIER = """#!/bin/bash
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
mode="$(cat "$out/mode")"
printf '%s\\n' "$$" > "$out/tier.pid"
if [[ "$mode" == missing ]]; then exit 0; fi
if [[ "$mode" == cancel ]]; then
  finish() { printf '{"owned_fixture_only":true}\\n' > "$out/receipt.json"; exit 130; }
  trap finish TERM
  : > "$out/ready"
  while :; do sleep 1 || true; done
fi
if [[ "$mode" == timeout ]]; then
  python3 - <<'PY'
import subprocess, sys
try: subprocess.run([sys.executable, '-c', 'import time; time.sleep(1)'], timeout=.05)
except subprocess.TimeoutExpired: raise SystemExit(124)
PY
fi
if [[ "$mode" != invalid ]]; then
  repo="$(cd "$(dirname "$0")/../.." && pwd)"
  commit="$(git -C "$repo" rev-parse HEAD)"
  exec /bin/bash --noprofile --norc -p "$repo/scripts/live-gates/t22-pair-queue-dispatch.sh" run "$out" "$repo" "$commit" "$out/input-manifest.tsv" "$out/hvf_gic_boot_probe"
fi
printf '{"owned_fixture_only":true}\\n' > "$out/receipt.json"
exit 0
"""

PREPARER = """#!/usr/bin/env python3
# Committed host-only producer substitute in the owned temporary repository.
import argparse, json, pathlib, sys
root = pathlib.Path(__file__).resolve().parents[2]
sys.path.insert(0, str(root / 'tests/integration'))
from t22_pair_worker_payloads import measured_core
parser = argparse.ArgumentParser()
for name in ('commit', 'job-id', 'input-sha256', 'origin-sha256', 'input-manifest', 'origin-manifest', 'output'):
    parser.add_argument('--' + name, required=True)
args = parser.parse_args()
directory = pathlib.Path(args.input_manifest).parent
mode = (directory / 'mode').read_text()
output = measured_core(directory, root, args.commit, mode)
assert output == pathlib.Path(args.output)
receipt = json.loads((output / 'preparation-receipt.json').read_bytes())
print(json.dumps(receipt, sort_keys=True))
raise SystemExit(0 if receipt['preparation_complete'] else 1)
"""


class WorkerFixture:
    def __init__(self, case):
        self.case = case
        self.temp = tempfile.TemporaryDirectory(prefix="bridgevm-t22-worker-")
        self.root = Path(self.temp.name).resolve()
        self.repo, self.queue, self.work = (self.root / name for name in ("repo", "queue", "work"))
        self.home = self.root / "home"; self.home.mkdir(mode=0o700)
        (self.home / "BridgeVM").mkdir(mode=0o700)
        self.children = []
        case.addCleanup(self.cleanup)
        shutil.copytree(LIVE, self.repo / "scripts/live-gates", ignore=shutil.ignore_patterns("__pycache__", "*.pyc"))
        shutil.copy2(ROOT / ".gitignore", self.repo / ".gitignore")
        for source in (ROOT / "scripts").glob("*.py"):
            shutil.copy2(source, self.repo / "scripts" / source.name)
        asset = Path("scripts/win-assets/bv-t22-pair-admission.ps1")
        (self.repo / asset.parent).mkdir(parents=True)
        shutil.copy2(ROOT / asset, self.repo / asset)
        tests = self.repo / "tests/integration"; tests.mkdir(parents=True)
        for name in ("t22_pair_fixtures.py", "t22_pair_worker_payloads.py"):
            shutil.copy2(ROOT / "tests/integration" / name, tests / name)
        for name, data in (("bridgevm-live", CLAIM), ("run-tier.sh", RUN_TIER),
                           ("prepare-t22-owned-pair.py", PREPARER)):
            path = self.repo / "scripts/live-gates" / name
            path.write_text(data); path.chmod(0o755)
        self.git("init", "-q")
        self.git("add", "-A")
        self.git("-c", "user.name=Owned Fixture", "-c", "user.email=fixture@example.invalid", "commit", "-qm", "owned fixture")
        self.commit = self.git("rev-parse", "HEAD").stdout.strip()
        for state in ("queued", "running", "done"):
            (self.queue / state).mkdir(parents=True)

    def git(self, *arguments):
        return subprocess.run(["/usr/bin/git", "-C", str(self.repo), *arguments],
                              env=self.env(), text=True, capture_output=True, check=True, timeout=20)

    def env(self):
        return dict(os.environ, HOME=str(self.home), BRIDGEVM_REPO=str(self.repo),
                    BRIDGEVM_LIVE_ROOT=str(self.queue), BRIDGEVM_LIVE_WORK=str(self.work),
                    BRIDGEVM_LIVE_MIN_FREE_GIB="0", GIT_CONFIG_GLOBAL=os.devnull, GIT_CONFIG_SYSTEM=os.devnull)

    def job(self, state="queued", name="a-fixture", mode="missing"):
        directory = self.queue / state / name
        directory.mkdir(mode=0o700)
        (directory / "job.env").write_text(f"job_id={name}\ntier={TIER}\ncommit={self.commit}\n")
        (directory / "mode").write_text(mode)
        return directory

    def sealed_job(self, state="queued", name="a-fixture", mode="missing"):
        from t22_pair_fixtures import PairFixture
        from t22_pair_queue_inputs import SCHEMA, seal
        if not hasattr(self, "pair"):
            retained = self.root / "retained"; retained.mkdir(mode=0o700)
            self.pair = PairFixture(retained)
        directory = self.job(state, name, mode)
        rows = {key: list(value) for key, value in self.pair.rows.items()}
        rows["source_commit"] = [self.commit]
        native = "".join("\t".join((key, *rows[key])) + "\n" for key in sorted(rows))
        query_hash = hashlib.sha256((self.repo / "scripts/win-assets/bv-t22-pair-admission.ps1").read_bytes()).hexdigest()
        data = (native + "schema\t" + SCHEMA + "\norigin_manifest\t" + str(self.pair.origin_path)
                + "\t" + self.pair.origin_hash + "\nquery_script_sha256\t" + query_hash + "\n").encode()
        source = directory / "input-manifest.tsv"; source.write_bytes(data); source.chmod(0o400)
        binary = directory / "hvf_gic_boot_probe"
        shutil.copy2(Path(rows["binary"][0]), binary); binary.chmod(0o400)
        seal(source, self.commit, directory, self.repo)
        identity = {"job_id": name, "tier": TIER, "commit": self.commit,
                    "input_manifest_sha256": hashlib.sha256(data).hexdigest(),
                    "sealed_binary_sha256": rows["binary"][1],
                    "origin_manifest_sha256": self.pair.origin_hash, "query_script_sha256": query_hash}
        env = "".join(key + "=" + value + "\n" for key, value in sorted(identity.items()))
        (directory / "job.env").write_text(env)
        ledger = self.queue / "job-ledger" / name; ledger.mkdir(parents=True, mode=0o700)
        (ledger / "entry.env").write_text(env); (ledger / "entry.env").chmod(0o400)
        return directory

    def worker(self):
        process = subprocess.Popen([str(self.repo / "scripts/live-gates/bridgevm-live-worker.sh")],
                                   env=self.env(), stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                                   start_new_session=True)
        self.children.append(process)
        return process

    def run(self, timeout=25, debug=False):
        process = self.worker()
        output = process.communicate(timeout=timeout)[0]
        output = output.decode(errors="replace")
        if debug and process.returncode:
            worktree = self.work / "a-fixture"
            running = self.queue / "running/a-fixture"
            script = "import pathlib,sys;sys.path.insert(0,sys.argv[2]+'/scripts/live-gates');from t22_pair_queue_receipt import collect;print(collect(pathlib.Path(sys.argv[1]),pathlib.Path(sys.argv[2]),sys.argv[3]))"
            details = subprocess.run([sys.executable, "-c", script, str(running), str(worktree), self.commit],
                                     env=self.env(), capture_output=True, text=True, timeout=15)
            output += details.stdout + details.stderr
            log = running / "preparer.stdout.private.log"
            if log.is_file(): output += log.read_bytes()[:8192].decode(errors="replace")
        return process.returncode, output

    def cancel(self):
        process = self.worker()
        running = self.queue / "running/a-fixture"
        deadline = time.monotonic() + 15
        while not (running / "ready").exists() and time.monotonic() < deadline:
            self.case.assertIsNone(process.poll(), "worker exited before owned tier was ready")
            time.sleep(.05)
        self.case.assertTrue((running / "ready").is_file())
        (running / "cancel.requested").touch()
        output = process.communicate(timeout=25)[0]
        return process.returncode, output.decode(errors="replace")

    def sealed_worktree(self, name="a-fixture"):
        self.work.mkdir(exist_ok=True)
        target = self.work / name
        self.git("worktree", "add", "--detach", str(target), self.commit)
        return target

    def recover(self):
        result = subprocess.run([str(self.repo / "scripts/live-gates/recover-stale-jobs.sh"),
                                 str(self.repo), str(self.queue), str(self.work)], env=self.env(),
                                stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=15)
        return result.returncode, result.stdout.decode(errors="replace")

    def emit(self, directory, worktree, mode):
        result = subprocess.run([sys.executable, "-B", str(worktree / "scripts/live-gates/t22_pair_queue.py"),
                                 "run", str(directory), str(worktree), self.commit,
                                 str(directory / "input-manifest.tsv"), str(directory / "hvf_gic_boot_probe")], env=self.env(),
                                stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=30)
        self.case.assertIn(result.returncode, (0, 1), result.stdout.decode(errors="replace"))

    def owned_group(self):
        process = subprocess.Popen([sys.executable, "-c", "import time; time.sleep(30)", str(self.root)],
                                   start_new_session=True, stdin=subprocess.DEVNULL,
                                   stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        self.children.append(process)
        return process.pid

    def assert_fenced(self, status, output, name="a-fixture", worktree=True):
        case = self.case
        case.assertEqual(status, 126, output)
        case.assertTrue((self.queue / "worker-cleanup-required").is_file(), output)
        case.assertTrue((self.queue / "running" / name).is_dir(), output)
        case.assertFalse((self.queue / "done" / name).exists(), output)
        if worktree: case.assertTrue((self.work / name).is_dir(), output)
        case.assertTrue((self.queue / "queued/z-next").is_dir(), output)

    def cleanup(self):
        for path in self.queue.rglob("tier.pid"):
            pid = int(path.read_text())
            command = subprocess.run(["/bin/ps", "-o", "command=", "-p", str(pid)],
                                     capture_output=True, text=True, timeout=5).stdout
            if str(self.root) in command and any(name in command for name in ("run-tier.sh", "t22_pair_queue.py")):
                try: os.killpg(pid, signal.SIGTERM)
                except ProcessLookupError: pass
        for process in self.children:
            if process.poll() is None:
                try: os.killpg(process.pid, signal.SIGTERM)
                except ProcessLookupError: pass
                try: process.communicate(timeout=5)
                except subprocess.TimeoutExpired:
                    try: os.killpg(process.pid, signal.SIGKILL)
                    except ProcessLookupError: pass
                    process.communicate(timeout=5)
        for path in self.root.rglob("*"):
            if not path.is_symlink(): path.chmod(0o700 if path.is_dir() else 0o600)
        self.temp.cleanup()
