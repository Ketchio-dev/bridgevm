"""Injected constructor failure after one disposable spawn; no VM launch."""
import argparse
from contextlib import ExitStack, redirect_stdout
import hashlib
import importlib.util
import io
import json
from pathlib import Path
import signal
import subprocess
import sys
from unittest.mock import patch

import guest_input_live_cleanup as cleanup

ROOT = Path(__file__).resolve().parents[2]
SHA = "a" * 40


def module(name):
    spec = importlib.util.spec_from_file_location(name, ROOT / "scripts/live-gates" / name)
    value = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(value)
    return value


def run_case(base, kind, failure, preflight=False, existing_work=False):
    runner = module("run-guest-input-live.py" if kind == "d5" else "run-b9-real-workload-pilot.py")
    source = base / "source"
    source.write_bytes(b"fixture")
    digest = hashlib.sha256(b"fixture").hexdigest()
    home = base / "home"
    parent = home / "BridgeVM/work"
    parent.mkdir(parents=True)
    owned, original_spawn, hashes_after_spawn = [], subprocess.Popen, []
    before_modes = []
    work = parent / ("guest-input-live-control" if kind == "d5" else "b9-pilot-control")
    sentinel, prior = work / "prior-lane-marker", None
    def fingerprint():
        return (work.stat().st_ino, work.stat().st_mode, sentinel.stat().st_ino,
                sentinel.stat().st_mode, sentinel.read_bytes())
    if existing_work:
        work.mkdir(mode=0o700)
        sentinel.write_bytes(b"owned stand-in for an unrelated prior lane")
        prior = fingerprint()
    paths = {key: source for key in ("image", "vars", "binary", "firmware")}
    hashes = {key: digest for key in paths}
    clones = {}
    def spawn(*args, **kwargs):
        before_modes.extend(p.stat().st_mode & 0o777 for p in clones.values())
        owned.append(original_spawn([sys.executable, "-c", "import time;time.sleep(15)"],
                                    start_new_session=True))
        raise failure
    def clone(*args):
        work.mkdir(exist_ok=True)
        for key in ("image", "vars"):
            clones[key] = work / key
            clones[key].write_bytes(b"fixture")
        return clones if kind == "d5" else (clones["image"], clones["vars"])
    def hash_file(path):
        if owned: hashes_after_spawn.append(str(path))
        return digest
    manifest = base / "manifest.json"
    manifest.write_text(json.dumps({"profile": "no-3d-installed-input-450s"}))
    previous = {s: signal.getsignal(s) for s in (signal.SIGTERM, signal.SIGINT)}
    try:
        with ExitStack() as stack:
            for obj, name, value in ((runner.platform, "system", "Darwin"),
                                     (runner.platform, "machine", "arm64"),
                                     (runner.subprocess, "check_output", SHA),
                                     (runner.Path, "home", home)):
                stack.enter_context(patch.object(obj, name, return_value=value))
            stack.enter_context(patch.object(runner.subprocess, "Popen", side_effect=spawn))
            stack.enter_context(patch.object(runner.subprocess, "run", side_effect=failure if preflight and kind == "d5" else None))
            if kind == "d5":
                stack.enter_context(patch.object(cleanup, "digest", side_effect=hash_file))
                stack.enter_context(patch.object(runner, "load", return_value=(paths, hashes, digest)))
                stack.enter_context(patch.object(runner, "clone_media", side_effect=clone))
                stack.enter_context(patch.object(runner, "stage", return_value=(source, {})))
                args = argparse.Namespace(commit=SHA, job_id="control", input_manifest=str(manifest))
                call = runner.execute
            else:
                out = base / "queued/control/diagnostic"
                out.mkdir(parents=True)
                job = {"job_id": "control", "commit": SHA, "input_manifest_sha256": digest,
                       "sealed_binary_sha256": digest, **{"asset_" + key + "_sha256": digest for key in runner.KEYS}}
                records = {key: (source, digest) for key in runner.KEYS}
                records["firmware"] = (source, digest)
                stack.enter_context(patch.object(runner, "sha", side_effect=hash_file))
                for name, value in (("job_fields", job), ("load_inputs", records),
                                    ("driver_umd_hash", digest), ("stage_external_pair", {}),
                                    ("stage_share", {})):
                    stack.enter_context(patch.object(runner, name, return_value=value))
                copy_raw = stack.enter_context(patch.object(runner, "copy_raw", return_value={}))
                stack.enter_context(patch.object(runner, "clone_pair", side_effect=clone))
                stack.enter_context(patch.object(runner, "verify_renderer_runtime", side_effect=failure if preflight else None))
                verify = stack.enter_context(patch.object(runner, "verify_inputs"))
                stack.enter_context(patch.object(runner, "check_shared_assets"))
                args = argparse.Namespace(out=out, job_id="control", input_manifest=source, sealed_binary=source)
                call = runner.run
            caught = None
            with redirect_stdout(io.StringIO()):
                try: call(args)
                except BaseException as error: caught = type(error).__name__
            receipt_path = work / "receipt.json" if kind == "d5" else out / "receipt.json"
            receipt = json.loads(receipt_path.read_text())
            return {"receipt": receipt, "spawned": len(owned), "caught": caught,
                    "owned_child_live": bool(owned) and owned[0].poll() is None,
                    "work_retained": work.exists(),
                    "clone_modes": [p.stat().st_mode & 0o777 for p in clones.values() if p.exists()],
                    "before_modes": before_modes, "hashes_after_spawn": hashes_after_spawn,
                    "prior_lane_unchanged": not existing_work or (work.exists() and sentinel.exists()
                        and prior == fingerprint()),
                    "copy_raw_calls": copy_raw.call_count if kind == "b9" else 0,
                    "verify_calls": verify.call_count if kind == "b9" else 0}
    finally:
        for process in owned:
            if process.poll() is None: process.kill()
            process.wait(timeout=5)
        for number, handler in previous.items(): signal.signal(number, handler)
