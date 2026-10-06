"""Private measured launch evidence; never proof of continuous process state."""
import hashlib
import json
import os

from guest_input_owned_group import state


class OwnedLaunch:
    def __init__(self, work, identity, receipt):
        self.fd = os.open(work, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
        info = os.fstat(self.fd)
        if (info.st_dev, info.st_ino) != identity or info.st_uid != os.geteuid():
            os.close(self.fd)
            raise ValueError("owned launch directory changed")
        self.base = {"commit": receipt["commit"], "job_id": receipt["job_id"],
                     "work_device": info.st_dev, "work_inode": info.st_ino}
        self.receipt, self.pid, self.verified, self.attempt_hash = receipt, None, False, None

    def write(self, filename, value):
        data = (json.dumps(value, sort_keys=True) + "\n").encode()
        fd = os.open(filename, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW,
                     0o600, dir_fd=self.fd)
        with os.fdopen(fd, "wb") as stream:
            stream.write(data); stream.flush(); os.fsync(stream.fileno())
        os.fsync(self.fd)
        return hashlib.sha256(data).hexdigest()

    def attempt(self):
        self.attempt_hash = self.write("owned-launch-attempt.json",
            {"schema": "bridgevm.t22-launch-attempt.v1", **self.base})
        self.receipt["owned_launch_attempt_sha256"] = self.attempt_hash

    def started(self, process):
        self.pid = process.pid
        if type(self.pid) is not int or self.pid < 2 or os.getpgid(self.pid) != self.pid:
            raise ValueError("owned boot lacks its new process group")
        self.verified = True

    def finish(self, process, cleanup):
        if self.attempt_hash is None:
            return
        pid = self.pid if self.pid is not None else (process.pid if process else None)
        terminal = process is not None and process.returncode is not None
        gone = self.verified and terminal and cleanup is True and state(pid) == "absent"
        if not gone: self.receipt["cleanup_complete"] = False
        value = {"schema": "bridgevm.t22-owned-launch.v1", **self.base,
                 "attempt_sha256": self.attempt_hash, "pid": pid,
                 "group_verified": self.verified, "terminal_observed": terminal,
                 "group_absence_observed": gone}
        self.receipt["owned_launch_context_sha256"] = self.write("owned-launch-context.json", value)

    def close(self):
        if self.fd is not None:
            os.close(self.fd)
            self.fd = None
