#!/usr/bin/env python3
"""Kill only one owned snapshot helper during staged real-media verification."""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import signal
import subprocess
import sys
import time

LSOF = "/usr/sbin/lsof"
FD = re.compile(r"f[0-9]+\Z")


def stable_root(disk: Path, vars: Path) -> Path:
    relative_vars = Path(os.path.relpath(vars, disk.parent))
    digest = hashlib.sha256()
    for value in (disk.name, str(relative_vars)):
        data = os.fsencode(value)
        digest.update(len(data).to_bytes(8, "little"))
        digest.update(data)
    return disk.parent / (".bridgevm-pair-v2-" + digest.hexdigest())


def regular(path: Path) -> bool:
    return path.is_file() and not path.is_symlink()


def private_new(path: Path, mode: str):
    descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
    return os.fdopen(descriptor, mode)


def read_fd_observed(data: str, pid: int, staged_disk: Path) -> bool:
    owner = False
    current_fd = ""
    access = ""
    for line in data.splitlines():
        if line.startswith("p"):
            owner = line == f"p{pid}"
            current_fd = ""
            access = ""
        elif line.startswith("f"):
            current_fd = line
            access = ""
        elif line.startswith("a"):
            access = line
        elif line.startswith("n") and owner and FD.fullmatch(current_fd) and access == "ar":
            if line[1:] == str(staged_disk):
                return True
    return False


def selected_old(root: Path) -> bool:
    current = root / "current"
    return not current.exists() and not current.is_symlink()


def stage_regular(root: Path) -> bool:
    stage = root / "staging"
    if not stage.is_dir() or stage.is_symlink():
        return False
    return all(regular(stage / name) for name in ("disk.raw", "vars.fd", "manifest.json"))


def stop_at_verified_stage(child: subprocess.Popen, root: Path, deadline: float,
                           record: Path) -> dict:
    stage_disk = root / "staging/disk.raw"
    while time.monotonic() < deadline:
        if child.poll() is not None:
            raise RuntimeError("restore helper exited before staged read")
        if stage_regular(root) and selected_old(root):
            observed = subprocess.run(
                [LSOF, "-n", "-P", "-p", str(child.pid), "-F", "pfan"],
                capture_output=True, text=True, timeout=3, check=False,
            )
            if observed.returncode == 0 and read_fd_observed(observed.stdout, child.pid, stage_disk):
                signal_pid = child.pid
                os.kill(signal_pid, signal.SIGSTOP)
                try:
                    stopped = os.waitpid(signal_pid, os.WUNTRACED | os.WNOHANG)
                    until = time.monotonic() + 3
                    while stopped == (0, 0) and time.monotonic() < until:
                        time.sleep(0.02)
                        stopped = os.waitpid(signal_pid, os.WUNTRACED | os.WNOHANG)
                    if stopped[0] != signal_pid or not os.WIFSTOPPED(stopped[1]):
                        raise RuntimeError("restore helper stop was not observed")
                    confirmed = subprocess.run(
                        [LSOF, "-n", "-P", "-p", str(signal_pid), "-F", "pfan"],
                        capture_output=True, text=True, timeout=3, check=False,
                    )
                    if (confirmed.returncode != 0 or
                        not read_fd_observed(confirmed.stdout, signal_pid, stage_disk) or
                        not stage_regular(root) or not selected_old(root)):
                        raise RuntimeError("staged read or old selection changed at stop")
                    with private_new(record, "w") as out:
                        out.write(confirmed.stdout)
                    return {"interruption_stage": "staged-disk-verify-read",
                            "helper_stop_verified": True,
                            "staged_file_sync_order_verified": True,
                            "old_selection_before_kill": True,
                            "stop_fd_log_sha256": hashlib.sha256(confirmed.stdout.encode()).hexdigest()}
                finally:
                    os.kill(signal_pid, signal.SIGKILL)
        time.sleep(0.05)
    raise TimeoutError("no authenticated staged read before deadline")


def run(helper: Path, snapshot: Path, disk: Path, vars: Path,
        output: Path, timeout_seconds: int) -> dict:
    if any(not regular(path) for path in (helper, disk, vars)) or not snapshot.is_dir():
        raise ValueError("restore inputs are not regular private files")
    disk = disk.resolve(strict=True)
    vars = vars.resolve(strict=True)
    snapshot = snapshot.resolve(strict=True)
    helper = helper.resolve(strict=True)
    root = stable_root(disk, vars)
    if root.joinpath("staging").exists() or root.joinpath("staging").is_symlink():
        raise ValueError("staging existed before helper launch")
    if not selected_old(root):
        raise ValueError("restore did not begin with old selected pair")
    stdout = output / "interrupt-helper.stdout"
    stderr = output / "interrupt-helper.stderr"
    with private_new(stdout, "wb") as out, private_new(stderr, "wb") as err:
        child = subprocess.Popen(
            [str(helper), "restore", str(snapshot), str(disk), str(vars)],
            cwd=helper.parent, env={"PATH": "/usr/bin:/bin:/usr/sbin:/sbin", "LANG": "C"},
            stdout=out, stderr=err,
        )
        stopped = None
        try:
            stopped = stop_at_verified_stage(
                child, root, time.monotonic() + timeout_seconds,
                output / "interrupt-helper-fd.private.log",
            )
        finally:
            if child.poll() is None:
                child.kill()
            code = child.wait(timeout=5)
    if stopped is None or code != -signal.SIGKILL or not selected_old(root):
        raise RuntimeError("restore helper was not killed before publication")
    stopped["helper_killed_and_reaped"] = True
    return stopped


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("helper", type=Path)
    parser.add_argument("snapshot", type=Path)
    parser.add_argument("disk", type=Path)
    parser.add_argument("vars", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--deadline", type=int, default=300)
    args = parser.parse_args()
    if not 1 <= args.deadline <= 300:
        parser.error("deadline must be 1..300 seconds")
    result = run(args.helper, args.snapshot, args.disk, args.vars,
                 args.output, args.deadline)
    result_path = args.output / "interrupt-observation.json"
    with private_new(result_path, "w") as out:
        json.dump(result, out, sort_keys=True)
        out.write("\n")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, ValueError, RuntimeError, TimeoutError,
            subprocess.SubprocessError) as error:
        print(f"FAIL: interrupted restore observation: {type(error).__name__}: {error}", file=sys.stderr)
        raise SystemExit(1)
