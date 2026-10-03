"""Keep the owned leader unreaped until the final nonzero group signal."""
import os
import signal
import subprocess
import time


def state(pgid):
    try:
        os.killpg(pgid, 0)
        return "present"
    except ProcessLookupError:
        return "absent"
    except PermissionError:
        return "denied"


def reap_and_check(process, timeout):
    try:
        process.wait(timeout=timeout)
    except subprocess.TimeoutExpired:
        return False
    # Darwin can return EPERM for a zombie-only group before its leader is
    # reaped. Neither EPERM nor a reaped leader proves group absence.
    return state(process.pid) == "absent"


def stop(process, grace=5):
    if process is None:
        return True
    pgid = process.pid
    if pgid <= 1 or pgid == os.getpgrp():
        raise ValueError("refusing to signal unowned process group")
    if process.returncode is not None:
        # The PID reservation has ended. Never signal a potentially reused
        # process group; an absent group is the only successful result.
        return state(pgid) == "absent"
    wait_timeout = max(.1, grace)
    for sig in (signal.SIGTERM, signal.SIGCONT):
        if process.returncode is not None:
            return state(pgid) == "absent"
        try:
            os.killpg(pgid, sig)
        except (ProcessLookupError, PermissionError):
            return reap_and_check(process, wait_timeout)
    deadline = time.monotonic() + grace
    while time.monotonic() < deadline:
        if state(pgid) != "present":
            return reap_and_check(process, wait_timeout)
        time.sleep(.05)
    # Do not poll or wait during the TERM grace: reaping would release the
    # leader's PID before this final signal.
    if process.returncode is not None:
        return state(pgid) == "absent"
    try:
        os.killpg(pgid, signal.SIGKILL)
    except (ProcessLookupError, PermissionError):
        pass
    return reap_and_check(process, wait_timeout)
