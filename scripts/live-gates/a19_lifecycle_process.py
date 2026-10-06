"""Bound lifecycle execution without treating its shell group as every descendant."""
from __future__ import annotations

import os
from pathlib import Path
import signal
import subprocess


class LifecycleCleanupUncertain(RuntimeError):
    """A stopped lifecycle cannot prove all job-control/auxiliary groups are gone."""


def alive(pgid: int) -> bool:
    try:
        os.killpg(pgid, 0)
    except ProcessLookupError:
        return False
    return True


def stop_root(child: subprocess.Popen, term_wait: float, kill_wait: float) -> None:
    if child.poll() is not None:
        return
    try:
        os.killpg(child.pid, signal.SIGTERM)
    except ProcessLookupError:
        pass
    try:
        child.wait(timeout=term_wait)
    except subprocess.TimeoutExpired:
        try:
            os.killpg(child.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        child.wait(timeout=kill_wait)


def run_lifecycle(repo: Path, environment: dict, deadline: float = 3600,
                  term_wait: float = 30, kill_wait: float = 5) -> int:
    child = None
    try:
        child = subprocess.Popen(
            [str(repo / "scripts/verify-native-snapshot-interrupted-restore.sh")],
            cwd=repo, env=environment, start_new_session=True,
        )
        try:
            status = child.wait(timeout=deadline)
        except subprocess.TimeoutExpired:
            stop_root(child, term_wait, kill_wait)
            # Job-control and auxiliary sessions can outlive the shell group.
            raise LifecycleCleanupUncertain("lifecycle deadline left descendant cleanup unproven")
        if alive(child.pid):
            raise LifecycleCleanupUncertain("lifecycle left its owned process group alive")
    except LifecycleCleanupUncertain:
        raise
    except BaseException as error:
        if child is not None:
            try:
                stop_root(child, term_wait, kill_wait)
            except BaseException:
                pass
        raise LifecycleCleanupUncertain("lifecycle ownership or teardown could not be proven") from error
    return status
