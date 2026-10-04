"""Bounded fixed preparer invocation; direct-child exit never proves native cleanup."""
import hashlib
import json
import signal
import subprocess
import sys

from t22_pair_publication import write_exclusive
from t22_pair_queue_inputs import stable_output
from t22_pair_environment import controlled_env


def command(directory, root, identity, native_hash):
    return [sys.executable, "-B", str(root / "scripts/live-gates/prepare-t22-owned-pair.py"),
            "--commit", identity["commit"], "--job-id", identity["job_id"],
            "--input-sha256", native_hash, "--origin-sha256", identity["origin_manifest_sha256"],
            "--input-manifest", str(directory / "native-candidate.tsv"),
            "--origin-manifest", str(directory / "retained-origin.json"),
            "--output", str(stable_output(identity["job_id"]))]


def command_hash(argv): return hashlib.sha256("\0".join(argv).encode()).hexdigest()


def store(path, value):
    raw = (json.dumps(value, sort_keys=True) + "\n").encode()
    write_exclusive(path, raw); path.chmod(0o400)
    return hashlib.sha256(raw).hexdigest()


def interrupted(signum, frame): raise InterruptedError("development preparation canceled")


def launch(argv, directory, identity, timeout=1800):
    base = {"commit": identity["commit"], "job_id": identity["job_id"], "command_sha256": command_hash(argv)}
    attempt = store(directory / "adapter-launch-attempt.json", {"schema": "bridgevm.t22-adapter-attempt.v1", **base})
    child, cause = None, "completed"
    handlers = {sig: signal.getsignal(sig) for sig in (signal.SIGTERM, signal.SIGINT)}
    try:
        for sig in handlers: signal.signal(sig, interrupted)
        with (directory / "preparer.stdout.private.log").open("xb") as log:
            child = subprocess.Popen(argv, env=controlled_env(), stdout=log, stderr=subprocess.STDOUT)
            child.wait(timeout=timeout)
    except subprocess.TimeoutExpired:
        cause = "timeout"
    except BaseException:
        cause = "canceled" if (directory / "cancel.requested").exists() else "interrupted"
    finally:
        for sig in handlers: signal.signal(sig, signal.SIG_IGN)
        try:
            if child is not None and child.poll() is None:
                child.terminate()
                try: child.wait(timeout=15)
                except subprocess.TimeoutExpired:
                    child.kill(); child.wait(timeout=5)
            if child is not None:
                store(directory / "adapter-launch-context.json", {"schema": "bridgevm.t22-adapter-launch.v1", **base,
                    "attempt_sha256": attempt, "core_pid": child.pid,
                    "terminal_observed": child.returncode is not None,
                    "exit_code": child.returncode, "cause": cause})
        finally:
            for sig, handler in handlers.items(): signal.signal(sig, handler)
    return child.returncode if child is not None and cause == "completed" else 1
