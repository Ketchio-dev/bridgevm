"""Finite owned process groups; never confuse a leader exit with cleanup."""
import os
from pathlib import Path
import shutil
import subprocess
import time

from guest_input_owned_group import stop, state
from d11_fixture_files import record
from t22_pair_environment import controlled_env

RESERVE = 104 << 30
OVERHEAD = 2 << 30


def capacity(free, gib):
    if type(gib) is not int or not 16 <= gib <= 32 or free - (gib << 30) - OVERHEAD < RESERVE:
        raise ValueError("fixture storage reserve refused")
    return gib << 30


class Processes:
    def __init__(self, directory, job, home):
        self.directory, self.job, self.home = directory, job, home
        self.process = None
        self.records = []
        self.cleanup_complete = True
        self.attempted = False
        self.uncertain = False

    def checkpoint(self):
        if (self.job / "cancel.requested").exists(): raise InterruptedError("fixture canceled")
        if shutil.disk_usage(self.home).free < RESERVE: raise OSError("fixture reserve reached")

    def launch(self, argv, log, env=None):
        self.checkpoint()
        if self.process is not None: raise ValueError("fixture process already owned")
        index = len(self.records)
        record(self.directory / f"process-{index}-attempt.private.json", {"argv": argv})
        self.attempted = True
        self.cleanup_complete = False
        with log.open("xb") as output:
            self.process = subprocess.Popen(argv, stdin=subprocess.DEVNULL, stdout=output,
                stderr=subprocess.STDOUT, start_new_session=True, env=env or controlled_env())
        record(self.directory / f"process-{index}-started.private.json", {"pid": self.process.pid})
        return self.process

    def wait(self, timeout):
        deadline = time.monotonic() + timeout
        while self.process.poll() is None:
            self.checkpoint()
            if time.monotonic() >= deadline: raise TimeoutError("fixture phase deadline")
            time.sleep(.25)
        return self.process.returncode

    def finish(self):
        process = self.process
        try:
            clean = False if self.attempted and process is None else stop(process)
        except BaseException:
            self.uncertain = True
            raise
        if process is not None:
            clean = clean and process.returncode is not None and state(process.pid) == "absent"
            item = {"pid": process.pid, "terminal_observed": process.returncode is not None,
                    "exit_code": process.returncode, "group_absent": clean}
            record(self.directory / f"process-{len(self.records)}-terminal.private.json", item)
            self.records.append(item)
        self.uncertain = self.uncertain or not clean
        self.cleanup_complete = not self.uncertain
        self.process = None
        self.attempted = False
        if not clean: raise ValueError("fixture group cleanup unproved")

    def run(self, argv, log, timeout=60, env=None):
        try:
            self.launch(argv, log, env)
            code = self.wait(timeout)
        finally:
            self.finish()
        if code != 0: raise ValueError("fixture command failed")


def quiet_host():
    # No argv/environment capture: executable basename is all this gate needs.
    output = subprocess.check_output(["/bin/ps", "-axo", "comm="], text=True, timeout=10,
                                     env=controlled_env())
    heavy = {"cargo", "rustc", "swift", "swiftc", "swift-frontend", "clang", "clang++",
             "hvf_gic_boot_probe", "qemu-system-aarch64", "xctest"}
    if any(Path(line.strip()).name in heavy for line in output.splitlines()):
        raise ValueError("fixture requires an idle native build and VM window")
    power = subprocess.check_output(["/usr/bin/pmset", "-g", "batt"], text=True, timeout=10,
                                    env=controlled_env())
    if "Now drawing from 'AC Power'" not in power: raise ValueError("fixture requires AC power")
