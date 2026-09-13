"""Bounded execution whose return boundary proves owned-process cleanup."""
import math
import signal
from subprocess import CompletedProcess, Popen, TimeoutExpired
from time import monotonic

from guest_input_live_cleanup import stop


class OwnedProcessError(RuntimeError):
    def __init__(self, message, *, cleanup_complete):
        if type(cleanup_complete) is not bool:
            raise TypeError("cleanup_complete must be a boolean")
        super().__init__(message)
        self.cleanup_complete = cleanup_complete


class _Cancellation(RuntimeError):
    pass


class _Signals:
    def __init__(self):
        self.previous = {}
        self.cancelled = None

    def record(self, number, _frame):
        # Never raise asynchronously: repeated signals must not interrupt stop.
        if self.cancelled is None:
            self.cancelled = number

    def install(self):
        for number in (signal.SIGTERM, signal.SIGINT):
            self.previous[number] = signal.getsignal(number)
            signal.signal(number, self.record)

    def restore(self):
        failure = None
        for number, previous in self.previous.items():
            try:
                signal.signal(number, previous)
            except BaseException as error:
                if failure is None:
                    failure = error
        return failure


def _wait(process, command, timeout, signals):
    deadline = monotonic() + timeout
    while True:
        if signals.cancelled is not None:
            raise _Cancellation("owned process execution canceled")
        remaining = deadline - monotonic()
        if remaining <= 0:
            raise TimeoutExpired(command, timeout)
        try:
            return process.wait(timeout=min(0.1, remaining))
        except TimeoutExpired:
            continue


def run_owned(command, *, timeout, stdout=None, stderr=None, cwd=None, env=None):
    process = None
    spawn_attempted = False
    returncode = None
    failure = None
    cleanup_failure = None
    signals = _Signals()
    try:
        if isinstance(timeout, bool) or not isinstance(timeout, (int, float)) or not math.isfinite(timeout) or timeout <= 0:
            raise ValueError("owned process timeout must be finite and positive")
        signals.install()
        if signals.cancelled is not None:
            raise _Cancellation("execution canceled before process creation")
        spawn_attempted = True
        process = Popen(command, stdout=stdout, stderr=stderr, cwd=cwd, env=env,
                        start_new_session=True)
        returncode = _wait(process, command, timeout, signals)
        if type(returncode) is not int:
            raise ValueError("owned process did not provide an integer exit status")
    except BaseException as error:
        failure = error

    # A constructor exception without a handle cannot prove process absence.
    # Never pass None to stop and treat its no-op as successful cleanup.
    cleanup_complete = not spawn_attempted
    if process is not None:
        try:
            cleanup_complete = stop(process) is True
            if not cleanup_complete:
                cleanup_failure = RuntimeError("owned process group absence is unproven")
        except BaseException as error:
            cleanup_complete = False
            cleanup_failure = error
    restore_failure = signals.restore()
    if signals.cancelled is not None and failure is None:
        failure = _Cancellation("owned process execution canceled")
    failure = failure or cleanup_failure or restore_failure
    if failure is not None or not cleanup_complete:
        message = (f"owned WinPE process failed: {type(failure).__name__}; "
                   f"cleanup_error={type(cleanup_failure).__name__}; "
                   f"restore_error={type(restore_failure).__name__}; "
                   f"cancellation_signal={signals.cancelled}")
        raise OwnedProcessError(message, cleanup_complete=cleanup_complete) from failure
    return CompletedProcess(command, returncode)
