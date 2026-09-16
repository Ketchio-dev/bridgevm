"""Bound the bare supervisor; its receipt must independently prove both LS exits."""
import os
import subprocess
import time

from app_ui_host_process import DEADLINE_SECONDS, request_cancel


def run_launcher(launcher, private, host_digest, launcher_digest, canceled):
    environment = {key: os.environ[key] for key in ("HOME", "USER", "LOGNAME") if key in os.environ}
    environment["PATH"] = "/usr/bin:/bin:/usr/sbin:/sbin"
    command = [str(launcher), "--app-ui-supervisor-v2", "--private-root", str(private),
               "--host-sha256", host_digest, "--launcher-sha256", launcher_digest]
    with (private / "launcher-v2.log").open("xb") as log:
        os.fchmod(log.fileno(), 0o600)
        child = subprocess.Popen(command, env=environment, stdout=log, stderr=subprocess.STDOUT)
        deadline = time.monotonic() + DEADLINE_SECONDS
        try:
            while child.poll() is None:
                if canceled() or time.monotonic() >= deadline:
                    request_cancel(private)
                    raise TimeoutError("dual-app diagnostic canceled or exceeded deadline")
                time.sleep(0.1)
            if canceled():
                request_cancel(private)
                raise TimeoutError("dual-app diagnostic canceled")
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
