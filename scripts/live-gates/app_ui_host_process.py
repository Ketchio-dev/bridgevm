"""Bounded launcher child lifetime, kept in the worker's owned process group."""
import os
import subprocess
import time

DEADLINE_SECONDS = 90


def request_cancel(private):
    try:
        with (private / "cancel.requested").open("xb"):
            pass
    except FileExistsError:
        pass


def run_launcher(launcher, bundle, observations, expected, receipt, canceled):
    private = observations.parent
    environment = {key: os.environ[key] for key in ("HOME", "USER", "LOGNAME") if key in os.environ}
    environment["PATH"] = "/usr/bin:/bin:/usr/sbin:/sbin"
    command = [str(launcher), "--app", str(bundle), "--output", str(observations),
               "--host-sha256", expected, "--receipt", str(receipt)]
    with (private / "launcher.log").open("xb") as log:
        child = subprocess.Popen(command, env=environment, stdout=log, stderr=subprocess.STDOUT)
        deadline = time.monotonic() + DEADLINE_SECONDS
        try:
            while child.poll() is None:
                if canceled() or time.monotonic() >= deadline:
                    request_cancel(private)
                    raise TimeoutError("normal-app diagnostic canceled or exceeded deadline")
                time.sleep(0.1)
            if canceled():
                request_cancel(private)
                raise TimeoutError("normal-app diagnostic canceled")
            return child.returncode
        finally:
            if child.poll() is None:
                request_cancel(private)
                child.terminate()
                try:
                    child.wait(timeout=12)
                except subprocess.TimeoutExpired:
                    child.kill()
                    child.wait(timeout=2)
