"""Bounded cleanup of a process group created and owned by this diagnostic."""
import json
import os
import signal
import time

from guest_input_live_inputs import digest


def group_alive(pgid):
    try:
        os.killpg(pgid, 0)
        return True
    except ProcessLookupError:
        return False


def stop(process, grace=5):
    if process is None:
        return True
    pgid = process.pid
    if pgid <= 1 or pgid == os.getpgrp():
        raise ValueError("refusing to signal unowned process group")
    for sig in (signal.SIGTERM, signal.SIGKILL):
        try:
            os.killpg(pgid, sig)
        except ProcessLookupError:
            process.poll()
            return True
        deadline = time.monotonic() + grace
        while time.monotonic() < deadline:
            process.poll()  # Reap the leader, but do not confuse it with the group.
            if not group_alive(pgid):
                return True
            time.sleep(0.05)
    process.poll()
    return not group_alive(pgid)


def finalize(receipt, process, paths, hashes, clones, work):
    receipt["complete"] = False
    try:
        receipt["cleanup_complete"] = stop(process)
    except (OSError, ValueError) as error:
        receipt["cleanup_complete"] = False
        receipt["cleanup_failure_type"] = type(error).__name__
    if receipt["cleanup_complete"]:
        try:
            receipt["source_integrity"] = all(digest(path) == hashes[name] for name, path in paths.items())
            receipt["output_hashes"] = {name: digest(path) for name, path in clones.items()}
            for path in clones.values():
                path.chmod(0o400)
            receipt["complete"] = True
        except (OSError, ValueError) as error:
            receipt["integrity_failure_type"] = type(error).__name__
    # Do not hash or chmod disks that a surviving process may still be writing.
    (work / "receipt.json").write_text(json.dumps(receipt, sort_keys=True) + "\n")
