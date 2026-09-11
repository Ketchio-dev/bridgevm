#!/usr/bin/env python3
"""Acknowledge a bounded synthetic survivor before its helper exits."""
import pathlib
import select
import subprocess
import sys

root = sys.argv[1]
assert pathlib.Path(root).is_dir()
code = "import time; print('BVSURVIVOR_READY', flush=True); time.sleep(20)"
child = subprocess.Popen(
    [sys.executable, "-c", code, root],
    stdout=subprocess.PIPE, start_new_session=True,
)
try:
    ready, _, _ = select.select([child.stdout], [], [], 5)
    if not ready or child.stdout.readline() != b"BVSURVIVOR_READY\n":
        raise RuntimeError("survivor did not acknowledge startup")
    observed = subprocess.run(
        ["/usr/bin/pgrep", "-f", root], capture_output=True, text=True, timeout=5,
    )
    print(f"survivor_pid={child.pid} observed_pids={observed.stdout.split()}", flush=True)
    if child.poll() is not None or str(child.pid) not in observed.stdout.split():
        raise RuntimeError("acknowledged survivor is not process-visible")
except BaseException:
    child.terminate()
    try:
        child.wait(timeout=5)
    except subprocess.TimeoutExpired:
        child.kill()
        child.wait()
    raise
finally:
    child.stdout.close()
