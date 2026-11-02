"""Owned signal drivers: readiness after handler install, then one real SIGTERM."""
import os
from pathlib import Path
import signal
import subprocess
import sys
import time


def swallowing_wait():
    # selectors.PollSelector.select returns on InterruptedError on every CPython,
    # and 3.15's macOS subprocess._wait_kqueue catches OSError around kqueue.control.
    # https://github.com/python/cpython/blob/v3.15.2/Lib/subprocess.py
    try:
        signal.getsignal(signal.SIGTERM)(signal.SIGTERM, None)
    except OSError:
        pass  # The helper resumes waiting instead of unwinding the operation.


def terminate_when_ready(arguments, ready, *, env=None, startup=30, limit=5):
    """Return the driver's exit status after TERM, or None if it outlived ``limit``."""
    driver = subprocess.Popen([sys.executable, "-B", *map(str, arguments)], env=env,
                              start_new_session=True)
    try:
        deadline = time.monotonic() + startup
        while not Path(ready).exists():
            if driver.poll() is not None or time.monotonic() >= deadline:
                raise AssertionError("signal fixture never became ready: %r" % driver.returncode)
            time.sleep(.02)
        driver.terminate()
        try:
            return driver.wait(timeout=limit)
        except subprocess.TimeoutExpired:
            return None
    finally:
        if driver.returncode is None:  # Still unreaped, so its group is still owned.
            os.killpg(driver.pid, signal.SIGKILL)
            driver.wait(timeout=5)
