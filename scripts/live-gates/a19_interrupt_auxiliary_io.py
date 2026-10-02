"""Bounded owned helper commands and independent auxiliary media authentication."""
from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path
import re
import signal
import stat
import subprocess
import time

from a19_interrupt_restore_child import private_new, read_fd_observed
from a19_interrupt_stop_points import regular, stable_root

DIGEST_FIELDS = ("disk_bytes", "disk_sha256", "vars_bytes", "vars_sha256")
MANIFEST_FIELDS = ("format_version", "vm_id", *DIGEST_FIELDS)
SHA256 = re.compile(r"[0-9a-f]{64}\Z")
MAX_RECORD = 64 * 1024


class CleanupUncertain(RuntimeError):
    """An owned process group could not be proven gone; media must remain fenced."""


def read_record(path: Path) -> bytes:
    with os.fdopen(os.open(path, os.O_RDONLY | os.O_NOFOLLOW), "rb") as source:
        if not stat.S_ISREG(os.fstat(source.fileno()).st_mode):
            raise ValueError("auxiliary record is not regular")
        value = source.read(MAX_RECORD + 1)
    if len(value) > MAX_RECORD:
        raise ValueError("auxiliary record is too large")
    return value


def strict_json(raw: bytes):
    def unique(pairs):
        result = {}
        for key, value in pairs:
            if key in result:
                raise ValueError("duplicate auxiliary JSON field")
            result[key] = value
        return result
    return json.loads(raw, object_pairs_hook=unique)


def content(path: Path) -> tuple[int, str]:
    with os.fdopen(os.open(path, os.O_RDONLY | os.O_NOFOLLOW), "rb") as source:
        before = os.fstat(source.fileno())
        if not stat.S_ISREG(before.st_mode) or before.st_size <= 0:
            raise ValueError("auxiliary media is not a nonempty regular file")
        digest = hashlib.sha256()
        while block := source.read(1024 * 1024):
            digest.update(block)
        after = os.fstat(source.fileno())
        if (before.st_size, before.st_mtime_ns) != (after.st_size, after.st_mtime_ns):
            raise ValueError("auxiliary media changed while hashing")
        return before.st_size, digest.hexdigest()


def pair(disk: Path, variables: Path) -> dict:
    result = {}
    for name, path in (("disk", disk), ("vars", variables)):
        result[name + "_bytes"], result[name + "_sha256"] = content(path)
    return result


def selected(disk: Path, variables: Path) -> tuple[Path, Path]:
    current = stable_root(disk, variables) / "current"
    if os.path.lexists(current):
        if current.is_symlink() or not current.is_dir():
            raise ValueError("auxiliary selected generation is invalid")
        return current / "disk.raw", current / "vars.fd"
    return disk, variables


def parsed(raw: bytes, fields: tuple[str, ...]) -> dict:
    lines = raw.decode("ascii").splitlines()
    if len(lines) != len(fields):
        raise ValueError("helper result field count is invalid")
    result = {}
    for field, line in zip(fields, lines):
        key, separator, value = line.partition(" ")
        if key != field or separator != " " or not value or " " in value:
            raise ValueError("helper result fields are invalid")
        if field.endswith("_bytes") or field == "format_version":
            if not re.fullmatch(r"[1-9][0-9]*", value) or int(value) >= 2**64:
                raise ValueError("helper result integer is invalid")
            result[field] = int(value)
        else:
            result[field] = value
    if any(not SHA256.fullmatch(result[field]) for field in fields if field.endswith("_sha256")):
        raise ValueError("helper result digest is invalid")
    return result


def manifest(snapshot: Path) -> dict:
    value = strict_json(read_record(snapshot / "manifest.json"))
    if (not isinstance(value, dict) or set(value) != set(MANIFEST_FIELDS) or
            type(value["format_version"]) is not int or value["format_version"] != 1 or
            not isinstance(value["vm_id"], str) or not re.fullmatch(r"[a-z0-9]+(-[a-z0-9]+)*", value["vm_id"])):
        raise ValueError("auxiliary manifest identity is invalid")
    expected = pair(snapshot / "disk.raw", snapshot / "vars.fd")
    if any(type(value[field]) is not type(expected[field]) or value[field] != expected[field]
           for field in DIGEST_FIELDS):
        raise ValueError("auxiliary manifest differs from independently hashed media")
    return value


def stop_proof(output: Path, point, staged_disk: Path) -> None:
    proof = strict_json(read_record(output / "interrupt-observation.json"))
    context = strict_json(read_record(output / "interrupt-helper-context.private.json"))
    raw = read_record(output / "interrupt-helper-fd.private.log")
    expected = {"interruption_stage", "helper_stop_verified", "staged_file_sync_order_verified",
                "old_selection_before_kill", "helper_killed_and_reaped", "stop_fd_log_sha256"}
    if (not isinstance(proof, dict) or set(proof) != expected or proof["interruption_stage"] != point.name or
            any(proof[field] is not True for field in expected - {"interruption_stage", "stop_fd_log_sha256"}) or
            proof["stop_fd_log_sha256"] != hashlib.sha256(raw).hexdigest()):
        raise ValueError("auxiliary stop observation is invalid")
    if (not isinstance(context, dict) or set(context) != {"helper_pid", "staged_disk_path", "selection_before", "selection_after"} or
            type(context["helper_pid"]) is not int or context["helper_pid"] <= 1 or
            context["staged_disk_path"] != str(staged_disk) or
            context["selection_before"] != context["selection_after"] or
            not isinstance(context["selection_before"], str) or
            (point.operation == "create" and context["selection_before"] != "absent") or
            not read_fd_observed(raw.decode("utf-8"), context["helper_pid"], staged_disk)):
        raise ValueError("auxiliary stop context differs from its exact owned staged read")


def stop_group(child: subprocess.Popen) -> None:
    if child.returncode is not None:
        try:
            os.killpg(child.pid, 0)
        except ProcessLookupError:
            return
        except PermissionError:
            raise CleanupUncertain("reaped auxiliary group liveness cannot be checked") from None
        raise CleanupUncertain("reaped auxiliary leader has an unverifiable residual group")
    for sig in (signal.SIGCONT, signal.SIGTERM):
        try:
            os.killpg(child.pid, sig)
        except (ProcessLookupError, PermissionError):
            pass
    until = time.monotonic() + 1
    while time.monotonic() < until:
        try:
            os.killpg(child.pid, 0)
        except (ProcessLookupError, PermissionError):
            break
        time.sleep(0.05)
    try:
        os.killpg(child.pid, signal.SIGKILL)
    except (ProcessLookupError, PermissionError):
        pass
    child.wait(timeout=2)
    until = time.monotonic() + 2
    while time.monotonic() < until:
        try:
            os.killpg(child.pid, 0)
        except ProcessLookupError:
            return
        except PermissionError:
            raise CleanupUncertain("auxiliary group liveness remains unproven after reap") from None
        time.sleep(0.05)
    raise CleanupUncertain("auxiliary helper process group cleanup is unproven")


class Commands:
    def __init__(self, budget: float, command_timeout: float = 300):
        self.deadline = time.monotonic() + budget
        self.command_timeout = command_timeout
        self.cleanup_verified = True

    def run(self, argv: list[str], stdout: Path, limit: float | None = None) -> bytes:
        timeout = min(limit or self.command_timeout, self.deadline - time.monotonic())
        if timeout <= 0:
            raise TimeoutError("auxiliary operation budget exhausted")
        with private_new(stdout, "wb") as out, private_new(stdout.with_suffix(stdout.suffix + ".stderr"), "wb") as err:
            child = None
            try:
                child = subprocess.Popen(argv, stdout=out, stderr=err, start_new_session=True,
                                         env={"PATH": "/usr/bin:/bin:/usr/sbin:/sbin", "LANG": "C"})
                code = child.wait(timeout=timeout)
            finally:
                if child is None:
                    self.cleanup_verified = False
                else:
                    try:
                        stop_group(child)
                    except BaseException:
                        self.cleanup_verified = False
                        raise
        if code != 0:
            raise RuntimeError("auxiliary helper command failed")
        return read_record(stdout)

    def clone(self, source: Path, destination: Path, output: Path) -> None:
        if os.path.lexists(destination) or not regular(source):
            raise ValueError("auxiliary clone must have a regular source and fresh destination")
        self.run(["/bin/cp", "-c", str(source), str(destination)], output)
        destination.chmod(destination.stat().st_mode | stat.S_IWUSR)

    def digest(self, helper: Path, disk: Path, variables: Path, output: Path, label: str) -> dict:
        raw = self.run([str(helper), "digest", str(disk), str(variables)], output / f"{label}-digest.txt")
        independent = pair(*selected(disk, variables))
        with private_new(output / f"{label}-host-digest.txt", "w") as host:
            host.write("".join(f"{field} {independent[field]}\n" for field in DIGEST_FIELDS))
        if parsed(raw, DIGEST_FIELDS) != independent:
            raise ValueError("helper selection differs from independent auxiliary hash")
        return independent
