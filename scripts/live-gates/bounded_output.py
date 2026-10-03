#!/usr/bin/env python3
"""Byte-preserving output with explicit, durable refusal instead of valid truncation."""
import argparse
import json
import os
from pathlib import Path
import select
import secrets
import stat
import sys
import tempfile
import threading

SCHEMA = "bridgevm.bounded-output.v1"
CHUNK_BYTES = 65536
STATUS_MAX_BYTES = 4096
DRAIN_TIMEOUT_SECONDS = 2.0
ERRORS = {"overflow", "input-io", "output-io", "status-io", "thread-error",
          "drain-timeout", "cancelled"}
KEYS = {"schema_version", "limit_bytes", "observed_bytes", "stored_bytes",
        "overflow", "complete", "error", "prepared", "owner_token"}


def _limit(value):
    if type(value) is not int or not 0 < value < 2**63:
        raise ValueError("invalid bounded-output limit")
    return value


def _token(value):
    if not isinstance(value, str) or not 1 <= len(value) <= 128 or any(
            c not in "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789._-" for c in value):
        raise ValueError("invalid bounded-output owner token")
    return value


def _path(value):
    path = Path(value).absolute()
    if ".." in path.parts or any(c in str(path) for c in "\0\r\n"):
        raise ValueError("invalid bounded-output path")
    parent = path.parent.resolve(strict=True)
    if not parent.is_dir():
        raise ValueError("invalid bounded-output parent")
    return parent / path.name


def _identity(info):
    if not stat.S_ISREG(info.st_mode) or info.st_nlink != 1 or info.st_uid != os.geteuid():
        raise ValueError("unsafe bounded-output file")
    return info.st_dev, info.st_ino


def _sync_parent(path):
    fd = os.open(path.parent, os.O_RDONLY | os.O_DIRECTORY)
    try:
        os.fsync(fd)
    finally:
        os.close(fd)


def _write(fd, data):
    view = memoryview(data)
    while view:
        count = os.write(fd, view)
        if count <= 0:
            raise OSError("bounded-output write failed")
        view = view[count:]


def _validate(value, limit_bytes, require_complete):
    if not isinstance(value, dict) or set(value) != KEYS or value.get("schema_version") != SCHEMA:
        raise ValueError("invalid bounded-output status")
    if type(value["limit_bytes"]) is not int or value["limit_bytes"] != _limit(limit_bytes):
        raise ValueError("bounded-output limit mismatch")
    _token(value["owner_token"])
    for key in ("observed_bytes", "stored_bytes"):
        if type(value[key]) is not int or not 0 <= value[key] < 2**64:
            raise ValueError("invalid bounded-output counts")
    for key in ("overflow", "complete", "prepared"):
        if type(value[key]) is not bool:
            raise ValueError("invalid bounded-output flags")
    if value["error"] is not None and value["error"] not in ERRORS:
        raise ValueError("invalid bounded-output error")
    if not value["prepared"] or value["stored_bytes"] > min(limit_bytes, value["observed_bytes"]):
        raise ValueError("invalid bounded-output storage")
    if value["overflow"] != (value["observed_bytes"] > limit_bytes):
        raise ValueError("invalid bounded-output overflow")
    valid = (value["error"] is None and not value["overflow"]
             and value["stored_bytes"] == value["observed_bytes"])
    if value["complete"] and not valid:
        raise ValueError("invalid bounded-output completion")
    if require_complete and (not value["complete"] or not valid):
        raise ValueError("bounded output refused or incomplete")
    return value


def _object(pairs):
    value = {}
    for key, item in pairs:
        if key in value:
            raise ValueError("duplicate bounded-output status field")
        value[key] = item
    return value


def read_status(path, limit_bytes, *, require_complete=True, output=None, owner_token=None):
    """Read strict small status; optionally verify the corresponding output size."""
    try:
        fd = os.open(_path(path), os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
        try:
            _identity(os.fstat(fd))
            data = os.read(fd, STATUS_MAX_BYTES + 1)
        finally:
            os.close(fd)
        if len(data) > STATUS_MAX_BYTES:
            raise ValueError("oversized bounded-output status")
        value = _validate(json.loads(data, object_pairs_hook=_object), limit_bytes, require_complete)
        if owner_token is not None and value["owner_token"] != _token(owner_token):
            raise ValueError("bounded-output ownership mismatch")
        if output is not None:
            info = _path(output).lstat()
            _identity(info)
            if info.st_size != value["stored_bytes"]:
                raise ValueError("bounded-output size mismatch")
        return value
    except (OSError, TypeError, json.JSONDecodeError, UnicodeError) as error:
        raise ValueError("unreadable bounded-output status") from error


class _Sink:
    def __init__(self, output, status, limit_bytes, owner_token):
        self.limit = _limit(limit_bytes)
        self.owner_token = _token(owner_token)
        self.output, self.status = _path(output), _path(status)
        if self.output == self.status:
            raise ValueError("bounded-output paths collide")
        self.lock = threading.Lock()
        self.observed = self.stored = 0
        self.error = None
        self.eof = False
        self.fd = None
        self.status_identity = None
        self.record = None
        try:
            flags = os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW
            self.fd = os.open(self.output, flags, 0o600)
            self.output_identity = _identity(os.fstat(self.fd))
            reserved = os.open(self.status, flags, 0o600)
            try:
                self.status_identity = _identity(os.fstat(reserved))
            finally:
                os.close(reserved)
            os.fsync(self.fd)
            _sync_parent(self.output)
            self.publish(False)
        except BaseException:
            if self.fd is not None:
                os.close(self.fd)
                self.fd = None
            raise

    def fail(self, category):
        with self.lock:
            if self.error is None:
                self.error = category

    def receive(self, data):
        with self.lock:
            self.observed += len(data)
            keep = min(len(data), max(0, self.limit - self.stored))
            writable = self.error in (None, "overflow")
        if writable and keep:
            try:
                view = memoryview(data)[:keep]
                while view:
                    count = os.write(self.fd, view)
                    if count <= 0:
                        raise OSError("bounded-output write failed")
                    with self.lock:
                        self.stored += count
                    view = view[count:]
            except BaseException:
                self.fail("output-io")
        with self.lock:
            if self.observed > self.limit and self.error is None:
                self.error = "overflow"

    def close_output(self):
        if self.fd is not None:
            try:
                os.fsync(self.fd)
            except BaseException:
                self.fail("output-io")
            finally:
                try:
                    os.close(self.fd)
                except BaseException:
                    self.fail("output-io")
                self.fd = None

    def publish(self, complete):
        with self.lock:
            value = dict(schema_version=SCHEMA, limit_bytes=self.limit,
                         observed_bytes=self.observed, stored_bytes=self.stored,
                         overflow=self.observed > self.limit, complete=complete,
                         error=self.error, prepared=True, owner_token=self.owner_token)
        _validate(value, self.limit, False)
        temporary = None
        try:
            if _identity(self.status.lstat()) != self.status_identity:
                raise ValueError("bounded-output status ownership changed")
            fd, temporary = tempfile.mkstemp(prefix=".bounded-output-", dir=self.status.parent)
            try:
                identity = _identity(os.fstat(fd))
                _write(fd, json.dumps(value, sort_keys=True).encode("ascii") + b"\n")
                os.fsync(fd)
            finally:
                os.close(fd)
            os.replace(temporary, self.status)
            temporary = None
            self.status_identity = identity
            try:
                _sync_parent(self.status)
            except BaseException:
                # An affirmative status with uncertain directory durability is invalid.
                if _identity(self.status.lstat()) == identity:
                    self.status.unlink()
                raise
            self.record = value
        except BaseException:
            self.fail("status-io")
            raise ValueError("bounded-output status persistence failed") from None
        finally:
            if temporary is not None:
                os.unlink(temporary)

    def finalize(self):
        try:
            info = self.output.lstat()
            if _identity(info) != self.output_identity or info.st_size != self.stored:
                raise ValueError("bounded-output evidence changed")
        except (OSError, ValueError):
            self.fail("output-io")
        with self.lock:
            complete = self.eof and self.error is None
        self.publish(complete)

    def check(self):
        value = read_status(self.status, self.limit, output=self.output,
                            owner_token=self.owner_token)
        try:
            if (value != self.record or _identity(self.status.lstat()) != self.status_identity
                    or _identity(self.output.lstat()) != self.output_identity):
                raise ValueError("bounded-output evidence changed")
        except OSError as error:
            raise ValueError("bounded-output evidence changed") from error
        return value


class BoundedOutput:
    """Subprocess stdout sink. Exit preserves caller exceptions; check after cleanup."""
    def __init__(self, output, status, limit_bytes, owner_token=None):
        token = secrets.token_hex(16) if owner_token is None else _token(owner_token)
        self.arguments = output, status, _limit(limit_bytes), token
        self.sink = self.thread = None
        self.write_fd = None
        self.cancel = threading.Event()
        self.finished = False
        self.failure = None

    def __enter__(self):
        if self.sink is not None:
            raise ValueError("bounded-output context already entered")
        self.sink = _Sink(*self.arguments)
        reader = None
        try:
            reader, self.write_fd = os.pipe()
            self.thread = threading.Thread(target=self._drain, args=(reader,), daemon=True)
            self.thread.start()
        except BaseException:
            if reader is not None:
                os.close(reader)
            if self.write_fd is not None:
                os.close(self.write_fd)
            self.write_fd = None
            self.sink.fail("thread-error")
            self.sink.close_output()
            self.sink.finalize()
            raise ValueError("bounded-output thread failed") from None
        return self

    def fileno(self):
        if self.write_fd is None:
            raise ValueError("bounded-output pipe is closed")
        return self.write_fd

    def _drain(self, reader):
        try:
            while not self.cancel.is_set():
                if not select.select([reader], [], [], 0.05)[0]:
                    continue
                data = os.read(reader, CHUNK_BYTES)
                if not data:
                    self.sink.eof = True
                    break
                self.sink.receive(data)
        except BaseException:
            self.sink.fail("input-io")
        finally:
            try:
                os.close(reader)
            except BaseException:
                self.sink.fail("input-io")
            finally:
                self.sink.close_output()

    def finish(self, *, check=True, cancelled=False):
        if self.sink is None:
            raise ValueError("bounded-output context was not entered")
        if cancelled:
            self.sink.fail("cancelled")
            if self.finished:
                try:
                    self.sink.finalize()
                except BaseException:
                    self.failure = "status-io"
        if not self.finished:
            if self.write_fd is not None:
                writer, self.write_fd = self.write_fd, None
                try:
                    os.close(writer)
                except BaseException:
                    self.sink.fail("input-io")
            self.thread.join(DRAIN_TIMEOUT_SECONDS)
            if self.thread.is_alive():
                self.sink.fail("drain-timeout")
                self.cancel.set()
                self.thread.join(0.2)
            try:
                self.sink.finalize()
            except BaseException:
                self.failure = "status-io"
            self.finished = True
        return self.check() if check else None

    def __exit__(self, kind, value, traceback):
        try:
            self.finish(check=False, cancelled=kind is not None)
        except BaseException:
            self.failure = "thread-error"
        return False

    def check(self):
        if not self.finished or self.failure or self.thread.is_alive():
            raise ValueError("bounded output refused or incomplete")
        return self.sink.check()


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--status", type=Path)
    parser.add_argument("--limit", type=int, required=True)
    parser.add_argument("--check-ready", type=Path)
    parser.add_argument("--token", required=True)
    args = parser.parse_args(argv)
    try:
        if args.check_ready is not None:
            if args.output is not None or args.status is not None:
                raise ValueError("invalid readiness arguments")
            value = read_status(args.check_ready, args.limit, require_complete=False, owner_token=args.token)
            if value["complete"] or value["observed_bytes"] or value["stored_bytes"] or value["overflow"] or value["error"]:
                raise ValueError("bounded-output reader is not ready")
            return 0
        if args.output is None or args.status is None:
            raise ValueError("bounded-output paths required")
        sink = _Sink(args.output, args.status, args.limit, args.token)
        try:
            while True:
                data = os.read(sys.stdin.fileno(), CHUNK_BYTES)
                if not data:
                    sink.eof = True
                    break
                sink.receive(data)
        except BaseException:
            sink.fail("input-io")
        finally:
            sink.close_output()
            sink.finalize()
        sink.check()
        return 0
    except (OSError, ValueError):
        print("bounded output refused", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
