"""Finite subprocesses; bounded in-memory drain, owned-group release, durable logs."""
import hashlib
import json
import os
from pathlib import Path
import select
import stat
import subprocess
import threading
import time


def save(path, value):
    data = (json.dumps(value, indent=2, sort_keys=True) + "\n").encode()
    if len(data) > 1024 * 1024:
        raise ValueError("oversize private record")
    write(path, data)


def write(path, data):
    fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
    try:
        view = memoryview(data)
        while view:
            count = os.write(fd, view)
            if count <= 0:
                raise OSError("short private write")
            view = view[count:]
        os.fsync(fd)
    finally:
        os.close(fd)
    fd = os.open(Path(path).parent, os.O_RDONLY | os.O_DIRECTORY)
    try:
        os.fsync(fd)
    finally:
        os.close(fd)


def content(path, maximum):
    fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
    try:
        before = os.fstat(fd)
        if not stat.S_ISREG(before.st_mode) or before.st_size > maximum or before.st_nlink != 1:
            raise ValueError("unsafe or oversized owned file")
        digest = hashlib.sha256()
        while block := os.read(fd, 1024 * 1024):
            digest.update(block)
        after = os.fstat(fd)
        if (before.st_size, before.st_mtime_ns) != (after.st_size, after.st_mtime_ns):
            raise ValueError("owned file changed while hashing")
        return {"dev": before.st_dev, "inode": before.st_ino, "bytes": before.st_size,
                "sha256": digest.hexdigest()}
    finally:
        os.close(fd)


class Reader:
    def __init__(self, file, limit):
        self.fd = os.dup(file.fileno())
        file.close()
        self.limit, self.observed, self.data = limit, 0, bytearray()
        self.eof, self.error = False, None
        self.cancel = threading.Event()
        self.thread = threading.Thread(target=self.drain, daemon=True)
        try:
            self.thread.start()
        except BaseException:
            os.close(self.fd)
            raise

    def drain(self):
        try:
            while not self.cancel.is_set():
                if not select.select([self.fd], [], [], .05)[0]:
                    continue
                block = os.read(self.fd, 65536)
                if not block:
                    self.eof = True
                    break
                self.observed += len(block)
                self.data.extend(block[:max(0, self.limit - len(self.data))])
        except BaseException as error:
            self.error = type(error).__name__
        finally:
            try:
                os.close(self.fd)
            except BaseException as error:
                self.error = type(error).__name__

    def finish(self, timeout):
        self.thread.join(timeout)
        if self.thread.is_alive():
            self.cancel.set()
            self.thread.join(.2)
        return {"observed": self.observed, "stored": len(self.data), "eof": self.eof,
                "error": self.error, "overflow": self.observed > self.limit,
                "complete": self.eof and not self.error and self.observed <= self.limit
                            and not self.thread.is_alive()}


class Commands:
    def __init__(self, root, stop, started):
        self.root, self.stop, self.started = root, stop, started
        self.deadline = started + 300
        self.normal_deadline = self.deadline - 60
        self.records, self.stored = [], 0
        self.cleanup_verified = True

    def run(self, argv, limit=30, *, cleanup=False, private_inventory=False, env=None, cwd=None):
        if len(self.records) >= 32:
            raise ValueError("finite command-count limit reached")
        until = self.deadline - 10 if cleanup else self.normal_deadline
        timeout = min(limit, until - time.monotonic() - 5)
        if timeout <= 0:
            raise TimeoutError("fixed fixture budget exhausted")
        cap = min(512 * 1024, (15 * 1024 * 1024 - self.stored) // 2)
        if cap <= 0:
            raise ValueError("total output quota exhausted")
        record = {"argv": argv, "cwd": str(cwd) if cwd else None,
                  "start_elapsed": time.monotonic() - self.started,
                  "timeout_seconds": timeout, "pid": None, "returncode": None}
        child = None
        readers = []
        error = None
        try:
            child = subprocess.Popen(argv, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE,
                                     stderr=subprocess.PIPE, start_new_session=True,
                                     env=env or {"PATH": "/usr/bin:/bin:/usr/sbin:/sbin", "LC_ALL": "C"}, cwd=cwd)
            record["pid"] = child.pid
            readers.append(Reader(child.stdout, cap))
            readers.append(Reader(child.stderr, cap))
            record["returncode"] = child.wait(timeout=timeout)
        except BaseException as caught:
            error = caught
            record["error"] = type(caught).__name__
        finally:
            if child is None:
                self.cleanup_verified = False
            else:
                try:
                    verified = self.stop(child, grace=.25)
                    self.cleanup_verified &= verified
                    record["group_cleanup_verified"] = verified
                except BaseException as caught:
                    self.cleanup_verified = False
                    record["group_cleanup_error"] = type(caught).__name__
            for name, reader in zip(("stdout", "stderr"), readers):
                state = reader.finish(max(0, min(2, self.deadline - time.monotonic())))
                state["sha256"] = hashlib.sha256(reader.data).hexdigest()
                record[name] = state
                self.stored += len(reader.data)
                if not private_inventory:
                    path = self.root / f"command-{len(self.records):02d}-{name}.log"
                    write(path, reader.data)
                if not state["complete"]:
                    error = error or ValueError("bounded output incomplete or overflowed")
            record["end_elapsed"] = time.monotonic() - self.started
            self.records.append(record)
            save(self.root / f"command-{len(self.records)-1:02d}.private.json", record)
        if error is not None or not self.cleanup_verified or len(readers) != 2:
            raise ValueError("owned command failed or cleanup uncertain") from error
        return record["returncode"], bytes(readers[0].data), bytes(readers[1].data)
